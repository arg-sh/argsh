//! Parser module — dispatch by file extension.

pub mod bash;
pub mod merge;
pub mod rust;

use crate::model::Document;
use anyhow::{anyhow, Result};
use std::path::Path;

/// Parse a source file into a Document based on its extension.
pub fn parse_file(path: &Path, content: &str) -> Result<Document> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("sh" | "bash" | "bats") => Ok(bash::parse(content)),
        Some("rs") => Ok(rust::parse(content, path)),
        _ => Err(anyhow!(
            "unsupported file type: {}",
            path.display()
        )),
    }
}
