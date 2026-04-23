// OCI Distribution client -- vendored and simplified from ocipkg
// (https://github.com/termoshtt/ocipkg, Apache-2.0 + MIT)
//
// Minimal sync client for pulling OCI artifacts from registries.
// No oci-spec dependency -- uses serde_json::Value for manifests.
// No async runtime -- uses ureq (blocking HTTP).

mod auth;
mod client;

pub use client::OciClient;
