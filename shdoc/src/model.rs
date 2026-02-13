//! Data model for parsed documentation — format-agnostic.

/// Complete parsed document from a single source file.
#[derive(Debug, Default)]
pub struct Document {
    pub file: FileDoc,
    pub functions: Vec<FunctionDoc>,
}

/// File-level metadata extracted from annotations.
#[derive(Debug, Default)]
pub struct FileDoc {
    /// @file / @name
    pub title: Option<String>,
    /// @brief
    pub brief: Option<String>,
    /// First @description before any function
    pub description: Option<String>,
    /// @tags (raw comma-separated string)
    pub tags: Option<String>,
}

/// A single documented function.
#[derive(Debug, Default)]
pub struct FunctionDoc {
    pub name: String,
    pub description: Option<String>,
    pub section: Option<SectionInfo>,
    pub example: Option<String>,
    /// @arg entries, sorted by zero-padded index
    pub args: Vec<ArgEntry>,
    pub noargs: bool,
    /// Well-formed @option entries
    pub options: Vec<OptionEntry>,
    /// Malformed @option entries (backward compat)
    pub options_bad: Vec<String>,
    /// @set entries (raw text)
    pub set_vars: Vec<String>,
    /// @exitcode entries (raw text)
    pub exit_codes: Vec<String>,
    /// @stdin entries (multi-line capable)
    pub stdin: Vec<String>,
    /// @stdout entries (multi-line capable)
    pub stdout: Vec<String>,
    /// @stderr entries (multi-line capable)
    pub stderr: Vec<String>,
    /// @see entries
    pub see_also: Vec<String>,
    /// Implementation sources (populated by merge)
    pub implementations: Vec<Implementation>,
}

#[derive(Debug, Default)]
pub struct SectionInfo {
    pub title: String,
    pub description: Option<String>,
}

/// Parsed @arg entry.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ArgEntry {
    /// Zero-padded number or "@" for sorting (used during parse-time sorting)
    pub sort_key: String,
    /// Raw text like "$1 string needle"
    pub raw: String,
}

/// Parsed well-formed @option entry.
#[derive(Debug)]
pub struct OptionEntry {
    /// e.g. "-o | --option \<arg\>"
    pub term: String,
    /// Description text
    pub definition: String,
}

/// Implementation language for cross-matching.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ImplLang {
    Bash,
    Rust,
}

/// Where a function is implemented.
#[derive(Debug, Clone)]
pub struct Implementation {
    pub lang: ImplLang,
    pub source_file: String,
}
