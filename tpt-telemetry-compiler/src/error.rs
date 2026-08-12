//! Errors produced while compiling a `.tpt-log` schema into a runtime parser.

/// Errors produced while compiling a schema.
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    /// A capture referenced a Grok pattern name the zero-copy matcher cannot
    /// expand to a native type. (Arbitrary Grok patterns are still supported by
    /// `tpt-grok-engine`; this compiler emits a structured, allocation-free
    /// matcher for the common native + mapped-Grok subset.)
    #[error("unsupported Grok pattern `{0}` in zero-copy compiler (use a native `%{{field:type}}` capture or `tpt-grok-engine`)")]
    UnsupportedGrok(String),

    /// A coercion referenced a field that is not produced by any capture.
    #[error("coercion/redaction references unknown field `{0}` in format `{1}`")]
    UnknownField(String, String),

    /// A type name could not be resolved.
    #[error("unknown scalar type `{0}`")]
    UnknownType(String),

    /// The generated code failed to render.
    #[error("codegen error: {0}")]
    Codegen(String),
}

impl From<tpt_telemetry_schema::SchemaError> for CompileError {
    fn from(e: tpt_telemetry_schema::SchemaError) -> Self {
        CompileError::Codegen(e.to_string())
    }
}

/// Convenience type alias for compiler results.
pub type Result<T> = std::result::Result<T, CompileError>;
