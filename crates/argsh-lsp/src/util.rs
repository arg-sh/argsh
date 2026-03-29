/// Extract the word (identifier with ::, - allowed) at the given column.
pub fn extract_word_at(line: &str, col: usize) -> String {
    if col >= line.len() {
        return String::new();
    }
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 {
        let ch = bytes[start - 1] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = col;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
            end += 1;
        } else {
            break;
        }
    }
    line[start..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_word() {
        assert_eq!(extract_word_at("echo hello", 5), "hello");
    }

    #[test]
    fn test_namespaced_function() {
        assert_eq!(extract_word_at("  main::serve() {", 7), "main::serve");
    }

    #[test]
    fn test_hyphenated_function() {
        assert_eq!(extract_word_at("  string::trim-left foo", 10), "string::trim-left");
    }

    #[test]
    fn test_at_boundary() {
        assert_eq!(extract_word_at("'port|p:~int'", 1), "port");
    }

    #[test]
    fn test_col_past_end() {
        assert_eq!(extract_word_at("hi", 99), "");
    }

    #[test]
    fn test_empty_line() {
        assert_eq!(extract_word_at("", 0), "");
    }
}
