/// Violation types for architecture linting
use std::path::PathBuf;

/// A single architecture linting violation
#[derive(Debug, Clone)]
pub struct Violation {
    /// Linting rule that was violated
    pub rule: String,
    /// Severity: error, warning
    pub severity: String,
    /// File containing violation
    pub file: PathBuf,
    /// Line number
    pub line: usize,
    /// Violation message
    pub message: String,
}

impl Violation {
    /// Create a new violation
    pub fn new(
        rule: impl Into<String>,
        severity: impl Into<String>,
        file: PathBuf,
        line: usize,
        message: impl Into<String>,
    ) -> Self {
        Violation {
            rule: rule.into(),
            severity: severity.into(),
            file,
            line,
            message: message.into(),
        }
    }
}
