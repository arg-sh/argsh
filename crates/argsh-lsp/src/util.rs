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
