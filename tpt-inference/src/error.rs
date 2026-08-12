//! Errors produced by inference providers.

use std::fmt;

/// Errors that can occur while suggesting or validating a schema.
#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    /// HTTP transport error talking to the provider.
    #[error("http error: {0}")]
    Http(String),
    /// The provider returned an API-level error (non-2xx with detail).
    #[error("provider api error: {0}")]
    Api(String),
    /// The suggested schema failed to parse as `.tpt-log`.
    #[error("invalid schema: {0}")]
    InvalidSchema(String),
    /// The suggested schema parsed but failed to compile (Phase 3).
    #[error("schema failed to compile: {0}")]
    Compile(String),
    /// Required API key environment variable was missing/empty.
    #[error("missing api key: set the `{0}` environment variable")]
    NoApiKey(String),
    /// JSON (de)serialization error.
    #[error("json error: {0}")]
    Json(String),
    /// The validation loop exhausted its retries.
    #[error("schema inference failed after retries: last error: {0}")]
    ValidationFailed(String),
    /// Any other provider-specific error.
    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for InferenceError {
    fn from(e: serde_json::Error) -> Self {
        InferenceError::Json(e.to_string())
    }
}

impl From<ureq::Error> for InferenceError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(_, resp) => {
                let body = resp.into_string().unwrap_or_default();
                InferenceError::Api(body)
            }
            other => InferenceError::Http(other.to_string()),
        }
    }
}

impl fmt::Display for AttemptContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "attempt {} failed: {}", self.attempt, self.error)
    }
}

/// A single prior attempt fed back to the model during the validation loop.
#[derive(Debug, Clone)]
pub struct AttemptContext {
    pub attempt: usize,
    pub error: String,
    pub schema_text: String,
}
