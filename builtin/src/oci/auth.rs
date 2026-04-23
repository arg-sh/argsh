// SPDX-License-Identifier: Apache-2.0 OR MIT
// Derived from ocipkg (https://github.com/termoshtt/ocipkg)
// OCI registry authentication -- simplified from ocipkg (Apache-2.0 + MIT)
//
// Handles bearer token challenge-response for ghcr.io, Harbor, ECR, etc.
// Reads ~/.docker/config.json for stored credentials.

use std::collections::HashMap;
use std::path::PathBuf;

/// Parsed WWW-Authenticate challenge from a 401 response.
#[derive(Debug)]
pub(crate) struct AuthChallenge {
    pub realm: String,
    pub service: String,
    pub scope: String,
}

impl AuthChallenge {
    /// Parse a `WWW-Authenticate` header value.
    ///
    /// Example:
    /// ```text
    /// Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:arg-sh/libs/data:pull"
    /// ```
    pub fn from_header(header: &str) -> Option<Self> {
        let (ty, params) = header.split_once(' ')?;
        if ty != "Bearer" {
            return None;
        }

        let mut realm = None;
        let mut service = None;
        let mut scope = None;
        // Parse key="value" pairs — split on commas outside quotes
        // (scope values like "repository:name:pull,push" contain commas)
        let mut rest = params;
        while !rest.is_empty() {
            rest = rest.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
            let Some((key, after_eq)) = rest.split_once('=') else { break };
            let key = key.trim();
            let (value, remainder) = if let Some(inner) = after_eq.strip_prefix('"') {
                // Quoted value — find closing quote
                if let Some(end) = inner.find('"') {
                    (&inner[..end], &inner[end + 1..])
                } else {
                    (inner, "")
                }
            } else {
                // Unquoted value — up to next comma
                after_eq.split_once(',').map_or((after_eq, ""), |(v, r)| (v, r))
            };
            match key {
                "realm" => realm = Some(value.to_string()),
                "service" => service = Some(value.to_string()),
                "scope" => scope = Some(value.to_string()),
                _ => {}
            }
            rest = remainder;
        }
        Some(Self {
            realm: realm?,
            service: service?,
            scope: scope?,
        })
    }

    /// Try to extract an `AuthChallenge` from a ureq 401 error.
    pub fn from_ureq_error(err: &ureq::Error) -> Option<Self> {
        match err {
            ureq::Error::Status(401, res) => {
                let header = res.header("www-authenticate")?;
                Self::from_header(header)
            }
            _ => None,
        }
    }
}

/// Load the base64-encoded `user:pass` credential for `domain` from
/// `~/.docker/config.json`.
pub(crate) fn docker_basic_auth(domain: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".docker/config.json");
    let content = std::fs::read_to_string(path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    config["auths"][domain]["auth"]
        .as_str()
        .map(|s| s.to_string())
}

/// Exchange credentials for a bearer token via the token endpoint.
pub(crate) fn fetch_token(
    agent: &ureq::Agent,
    challenge: &AuthChallenge,
    basic_auth: Option<&str>,
    registry: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut req = agent
        .get(&challenge.realm)
        .set("Accept", "application/json")
        .query("service", &challenge.service)
        .query("scope", &challenge.scope);

    // Only send credentials if realm host matches the registry
    let send_creds = basic_auth.and_then(|cred| {
        let realm_host = challenge.realm.split('/').nth(2).unwrap_or("");
        if realm_host == registry || realm_host.ends_with(&format!(".{}", registry)) {
            Some(cred)
        } else {
            None
        }
    });
    if let Some(cred) = send_creds {
        req = req.set("Authorization", &format!("Basic {}", cred));
    }

    let body: HashMap<String, serde_json::Value> = req.call()?.into_json()?;

    let token = body
        .get("token")
        .or_else(|| body.get("access_token"))
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or("no token field in token response")?;

    Ok(token.to_string())
}
