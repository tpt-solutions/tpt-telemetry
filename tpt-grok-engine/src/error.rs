//! Error types for the Grok engine.

use std::fmt;

/// Errors produced while compiling or running Grok patterns.
#[derive(Debug)]
pub enum GrokError {
    /// A `%{ ... }` was not closed by a matching `}`.
    UnbalancedBrace(String),
    /// A capture had more than the supported `name:field:type` shape.
    BadCapture(String),
    /// Referenced a Grok pattern name that is not in the standard library.
    UnknownPattern(String),
    /// Recursive pattern expansion exceeded the depth limit.
    TooDeep(String),
    /// Pattern expansion detected a reference cycle.
    Cycle(String),
    /// A capture field name was not a valid regex group name.
    InvalidGroupName(String),
    /// The final compiled regex was invalid.
    Regex(regex::Error),
}

impl fmt::Display for GrokError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GrokError::UnbalancedBrace(p) => write!(f, "unbalanced `%{{` in pattern `{p}`"),
            GrokError::BadCapture(c) => write!(f, "malformed capture `{c}`"),
            GrokError::UnknownPattern(n) => write!(f, "unknown Grok pattern `{n}`"),
            GrokError::TooDeep(n) => write!(f, "pattern expansion too deep at `{n}`"),
            GrokError::Cycle(n) => write!(f, "pattern cycle detected at `{n}`"),
            GrokError::InvalidGroupName(n) => write!(f, "invalid capture group name `{n}`"),
            GrokError::Regex(e) => write!(f, "invalid regex: {e}"),
        }
    }
}

impl std::error::Error for GrokError {}

impl From<regex::Error> for GrokError {
    fn from(e: regex::Error) -> Self {
        GrokError::Regex(e)
    }
}
