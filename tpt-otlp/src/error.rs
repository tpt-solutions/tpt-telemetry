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
    /// Auth/secret headers were supplied over a plaintext (`http://`) transport
    /// while `require_tls` was set, risking credential leakage.
    #[error("auth headers sent over insecure transport: {0}")]
    InsecureTransport(String),
    /// A log record carried a timestamp that could not be parsed as nanoseconds.
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
}
