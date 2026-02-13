//! Renderer module — trait-based format dispatch.

pub mod html;
pub mod json;
pub mod markdown;

use crate::model::Document;
use anyhow::{anyhow, Result};

/// Trait for rendering a Document into a specific output format.
pub trait Renderer {
    fn render(&self, doc: &Document) -> String;
    fn file_extension(&self) -> &str;
}

/// Create a renderer for the given format name.
pub fn create_renderer(format: &str) -> Result<Box<dyn Renderer>> {
    match format {
        "markdown" | "md" => Ok(Box::new(markdown::MarkdownRenderer)),
        "html" => Ok(Box::new(html::HtmlRenderer)),
        "json" => Ok(Box::new(json::JsonRenderer)),
        _ => Err(anyhow!(
            "unknown format: {}. Use markdown, html, or json",
            format
        )),
    }
}
