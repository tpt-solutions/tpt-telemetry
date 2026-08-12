//! OTLP export errors.

/// Errors produced while mapping or exporting records to OTLP.
#[derive(Debug, thiserror::Error)]
pub enum OtlpError {
    /// Serialization (JSON) failure.
    #[error("serialize error: {0}")]
    Serialize(String),
    /// Transport failure (HTTP/gRPC).
    #[error("transport error: {0}")]
    Transport(String),
    /// The gRPC transport was selected but the `grpc` feature is disabled.
    #[error("gRPC transport requested but the `grpc` feature is not enabled")]
    GrpcFeatureDisabled,
    /// All retry attempts were exhausted.
    #[error("export failed after {0} retries: {1}")]
    RetriesExhausted(usize, String),
}
