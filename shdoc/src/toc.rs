//! GitHub-flavored markdown anchor/slug generation.
//!
//! Mirrors the `render_toc_link()` function from the gawk shdoc (lines 149-186).

/// Generate a table-of-contents link for a function name.
pub fn render_toc_link(text: &str) -> String {
    // Relative links (starting with /, ./, ../)
    if text.starts_with('/') || text.starts_with("./") || text.starts_with("../") {
        return format!("[{}]({})", text, text);
    }

    // Already a markdown link
    if contains_markdown_link(text) {
        return text.to_string();
    }

    // Check for bare URLs and wrap them
    if contains_bare_url(text) {
        return wrap_bare_urls(text);
    }

    // Generate GitHub anchor slug
    let slug = github_slug(text);
    format!("[{}](#{})", text, slug)
}

/// Generate a TOC list item.
pub fn render_toc_item(title: &str) -> String {
    format!("* {}", render_toc_link(title))
}

/// GitHub heading anchor slug generation.
///
/// Matches the algorithm at:
/// https://github.com/jch/html-pipeline/blob/master/lib/html/pipeline/toc_filter.rb#L44-L45
///
/// And the gawk implementation (shdoc lines 177-185):
/// - lowercase
/// - remove all chars that aren't alphanumeric, space, underscore, or hyphen
/// - remove underscores (separately from the char filter)
/// - replace spaces with hyphens
fn github_slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for c in text.to_lowercase().chars() {
        if c.is_alphanumeric() || c == ' ' || c == '-' {
            slug.push(c);
        }
        // All other chars (including ':', '.', '_') are stripped
    }
    slug.replace(' ', "-")
}

/// Check if text contains a markdown link `[...](...)`.
fn contains_markdown_link(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Find closing ]
            if let Some(close) = text[i + 1..].find(']') {
                let after = i + 1 + close + 1;
                if after < bytes.len() && bytes[after] == b'(' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// Check if text contains a bare URL (not already in a markdown link).
fn contains_bare_url(text: &str) -> bool {
    // Simple heuristic: contains "://" but not preceded by "]("
    if let Some(pos) = text.find("://") {
        // Check it's not inside a markdown link
        if pos >= 2 {
            let before = &text[..pos];
            if before.ends_with("](") || before.contains("](") {
                return false;
            }
        }
        return true;
    }
    false
}

/// Wrap bare URLs in markdown link syntax.
fn wrap_bare_urls(text: &str) -> String {
    // Simple approach: find URLs and wrap them
    // The gawk version is more complex, but this handles the common cases
    let mut result = text.to_string();

    // Find URL-like patterns
    let url_start_patterns = ["http://", "https://", "ftp://"];
    for pattern in &url_start_patterns {
        if let Some(start) = result.find(pattern) {
            // Find end of URL (first whitespace or end of string)
            let url_part = &result[start..];
            let end = url_part
                .find(|c: char| c.is_whitespace())
                .unwrap_or(url_part.len());
            let url = &result[start..start + end].to_string();
            let replacement = format!("[{}]({})", url, url);
            result = result.replacen(url.as_str(), &replacement, 1);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_simple() {
        assert_eq!(github_slug("hello world"), "hello-world");
    }

    #[test]
    fn slug_with_colons() {
        assert_eq!(github_slug("string::trim"), "stringtrim");
        assert_eq!(github_slug("is::array"), "isarray");
        assert_eq!(github_slug("to::int"), "toint");
    }

    #[test]
    fn slug_with_hyphens() {
        assert_eq!(github_slug("string::drop-index"), "stringdrop-index");
        assert_eq!(github_slug("string::trim-left"), "stringtrim-left");
    }

    #[test]
    fn slug_uppercase() {
        assert_eq!(github_slug("Hello World"), "hello-world");
    }

    #[test]
    fn toc_link_function() {
        assert_eq!(
            render_toc_link("string::trim"),
            "[string::trim](#stringtrim)"
        );
    }

    #[test]
    fn toc_link_relative() {
        assert_eq!(
            render_toc_link("./other"),
            "[./other](./other)"
        );
    }

    #[test]
    fn toc_item() {
        assert_eq!(
            render_toc_item("is::array"),
            "* [is::array](#isarray)"
        );
    }
}
