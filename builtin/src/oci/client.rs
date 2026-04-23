// SPDX-License-Identifier: Apache-2.0 OR MIT
// Derived from ocipkg (https://github.com/termoshtt/ocipkg)
// OCI Distribution client -- simplified from ocipkg (Apache-2.0 + MIT)
//
// Provides: get_manifest, get_blob, push_blob, push_manifest.  Sync only.

use super::auth::{self, AuthChallenge};
use std::io::Read;

type BoxErr = Box<dyn std::error::Error>;

/// Lightweight descriptor extracted from a manifest JSON.
#[derive(Debug, Clone)]
pub struct Descriptor {
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    pub annotations: Option<serde_json::Value>,
}

/// Minimal result of parsing an OCI image manifest (or Docker v2s2).
#[derive(Debug)]
pub struct Manifest {
    pub media_type: Option<String>,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
}

/// Sync OCI distribution client (pull + push).
pub struct OciClient {
    agent: ureq::Agent,
    registry: String,
    name: String,
    reference: String,
    token: Option<String>,
    basic_auth: Option<String>,
}

impl OciClient {
    /// Create a new client for `registry/name:reference`.
    ///
    /// ```ignore
    /// let mut c = OciClient::new("ghcr.io", "arg-sh/libs/data", "0.1.0")?;
    /// ```
    pub fn new(registry: &str, name: &str, reference: &str) -> Result<Self, BoxErr> {
        let basic_auth = auth::docker_basic_auth(registry);
        Ok(Self {
            agent: ureq::Agent::new(),
            registry: registry.to_string(),
            name: name.to_string(),
            reference: reference.to_string(),
            token: None,
            basic_auth,
        })
    }

    // -- internal helpers ---------------------------------------------------

    /// Build a full URL under `/v2/<name>/...`.
    fn url(&self, path: &str) -> String {
        format!("https://{}/v2/{}/{}", self.registry, self.name, path)
    }

    /// Ensure we have a valid token with the required scope.
    /// Triggers a challenge-response against the given URL if needed.
    fn ensure_auth(&mut self, url: &str) -> Result<(), BoxErr> {
        if self.token.is_some() {
            return Ok(());
        }
        // Probe to get the 401 challenge.
        match self.agent.get(url).call() {
            Ok(_) => Ok(()),
            Err(ref e) => {
                if let Some(challenge) = AuthChallenge::from_ureq_error(e) {
                    let tok = auth::fetch_token(
                        &self.agent,
                        &challenge,
                        self.basic_auth.as_deref(),
                        &self.registry,
                    )?;
                    self.token = Some(tok);
                    Ok(())
                } else {
                    Err(format!("registry request failed: {e}").into())
                }
            }
        }
    }

    /// Perform an authenticated GET, handling a single 401 challenge-response
    /// round-trip transparently.
    fn authed_get(&mut self, url: &str, accept: Option<&str>) -> Result<ureq::Response, BoxErr> {
        // If we already have a token, try it first.
        if let Some(tok) = &self.token {
            let mut req = self.agent.get(url).set("Authorization", &format!("Bearer {}", tok));
            if let Some(a) = accept {
                req = req.set("Accept", a);
            }
            match req.call() {
                Ok(res) => return Ok(res),
                // Token expired / wrong scope -- fall through to re-auth.
                Err(ureq::Error::Status(401, _)) => {
                    self.token = None;
                }
                Err(e) => return Err(e.into()),
            }
        }

        // First request (unauthenticated) to get the challenge.
        let mut req = self.agent.get(url);
        if let Some(a) = accept {
            req = req.set("Accept", a);
        }
        match req.call() {
            Ok(res) => return Ok(res),
            Err(ref e) => {
                if let Some(challenge) = AuthChallenge::from_ureq_error(e) {
                    let tok = auth::fetch_token(
                        &self.agent,
                        &challenge,
                        self.basic_auth.as_deref(),
                        &self.registry,
                    )?;
                    self.token = Some(tok);
                } else {
                    // Not a 401 -- propagate.
                    return Err(format!("registry request failed: {e}").into());
                }
            }
        }

        // Retry with the freshly obtained token.
        let tok = self.token.as_ref().ok_or("auth: no token after challenge")?;
        let mut req = self.agent.get(url).set("Authorization", &format!("Bearer {}", tok));
        if let Some(a) = accept {
            req = req.set("Accept", a);
        }
        Ok(req.call()?)
    }

    // -- public API ---------------------------------------------------------

    /// Fetch and parse the image manifest for the configured reference (tag or
    /// digest).  Works with both OCI image manifests and Docker v2s2.
    pub fn get_manifest(&mut self) -> Result<Manifest, BoxErr> {
        let url = self.url(&format!("manifests/{}", self.reference));
        let accept = [
            "application/vnd.oci.image.manifest.v1+json",
            "application/vnd.docker.distribution.manifest.v2+json",
        ]
        .join(", ");

        let res = self.authed_get(&url, Some(&accept))?;
        let val: serde_json::Value = res.into_json()?;
        parse_manifest(&val)
    }

    /// Download a blob by its digest (e.g. `sha256:abcdef...`).
    pub fn get_blob(&mut self, digest: &str) -> Result<Vec<u8>, BoxErr> {
        let url = self.url(&format!("blobs/{}", digest));
        let res = self.authed_get(&url, None)?;
        let mut bytes = Vec::new();
        res.into_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Resolve the current reference to its content digest by issuing a HEAD
    /// (or GET) against the manifests endpoint and reading `Docker-Content-Digest`.
    pub fn resolve_digest(&mut self) -> Result<String, BoxErr> {
        let url = self.url(&format!("manifests/{}", self.reference));
        let accept = [
            "application/vnd.oci.image.manifest.v1+json",
            "application/vnd.docker.distribution.manifest.v2+json",
        ]
        .join(", ");

        let res = self.authed_get(&url, Some(&accept))?;
        res.header("Docker-Content-Digest")
            .map(|s| s.to_string())
            .ok_or_else(|| "registry did not return Docker-Content-Digest header".into())
    }

    // -- push API -----------------------------------------------------------

    /// Upload a blob to the registry.  Returns its `sha256:<hex>` digest.
    ///
    /// Implements the OCI blob upload flow:
    /// 1. `POST /v2/<name>/blobs/uploads/` → get upload URL from `Location` header.
    /// 2. `PUT <location>?digest=sha256:...` with the blob body (monolithic upload).
    pub fn push_blob(&mut self, data: &[u8]) -> Result<String, BoxErr> {
        use sha2::{Digest, Sha256};
        let digest = format!("sha256:{:x}", Sha256::digest(data));

        // Check if blob already exists (HEAD).
        let head_url = self.url(&format!("blobs/{}", digest));
        self.ensure_auth(&head_url)?;
        if let Some(tok) = &self.token {
            if let Ok(res) = self.agent.head(&head_url)
                .set("Authorization", &format!("Bearer {}", tok))
                .call()
            {
                if res.status() == 200 {
                    return Ok(digest);
                }
            }
        }

        // Initiate upload.
        let upload_url = self.url("blobs/uploads/");
        self.ensure_auth(&upload_url)?;
        let tok = self.token.clone()
            .ok_or("push_blob: no auth token available after ensure_auth")?;
        let res = self.agent.post(&upload_url)
            .set("Authorization", &format!("Bearer {}", tok))
            .set("Content-Length", "0")
            .call()?;

        let location = res.header("Location")
            .ok_or("push_blob: no Location header from POST")?
            .to_string();

        // Resolve relative Location against registry.
        let put_url = if location.starts_with("http") {
            location
        } else {
            format!("https://{}{}", self.registry, location)
        };

        // Append digest query parameter.
        let separator = if put_url.contains('?') { "&" } else { "?" };
        let put_url = format!("{}{}digest={}", put_url, separator, digest);

        // Monolithic PUT with full blob body.
        self.agent.put(&put_url)
            .set("Authorization", &format!("Bearer {}", tok))
            .set("Content-Type", "application/octet-stream")
            .set("Content-Length", &data.len().to_string())
            .send_bytes(data)?;

        Ok(digest)
    }

    /// Push a manifest to the registry under the configured reference (tag).
    /// Returns the manifest digest.
    pub fn push_manifest(&mut self, manifest_json: &[u8]) -> Result<String, BoxErr> {
        let url = self.url(&format!("manifests/{}", self.reference));
        self.ensure_auth(&url)?;
        let tok = self.token.clone()
            .ok_or("push_manifest: no auth token available after ensure_auth")?;

        let res = self.agent.put(&url)
            .set("Authorization", &format!("Bearer {}", tok))
            .set("Content-Type", "application/vnd.oci.image.manifest.v1+json")
            .set("Content-Length", &manifest_json.len().to_string())
            .send_bytes(manifest_json)?;

        let digest = res.header("Docker-Content-Digest")
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                use sha2::{Digest, Sha256};
                format!("sha256:{:x}", Sha256::digest(manifest_json))
            });
        Ok(digest)
    }
}

// -- manifest parsing -------------------------------------------------------

fn parse_descriptor(v: &serde_json::Value) -> Result<Descriptor, BoxErr> {
    Ok(Descriptor {
        media_type: v["mediaType"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_string(),
        digest: v["digest"]
            .as_str()
            .ok_or("descriptor missing digest")?
            .to_string(),
        size: v["size"].as_u64().unwrap_or(0),
        annotations: v.get("annotations").cloned(),
    })
}

fn parse_manifest(val: &serde_json::Value) -> Result<Manifest, BoxErr> {
    let media_type = val["mediaType"].as_str().map(|s| s.to_string());
    let config = parse_descriptor(&val["config"])?;
    let layers = val["layers"]
        .as_array()
        .ok_or("manifest missing layers array")?
        .iter()
        .map(parse_descriptor)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Manifest {
        media_type,
        config,
        layers,
    })
}
