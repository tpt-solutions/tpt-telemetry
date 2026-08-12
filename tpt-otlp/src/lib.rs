//! `tpt-otlp` — typed-log → OTLP log record mapping and exporters.
//!
//! Provides:
//! - an internal data model mirroring the OTLP/JSON logs schema ([`model`]),
//! - a typed [`Record`](tpt_telemetry_compiler::Record) → OTLP converter,
//! - an [`Exporter`] with config-driven transport selection (`HTTP` or `gRPC`),
//!   batching, and retry/backoff.
//!
//! The HTTP/JSON path works out of the box. The gRPC path requires the `grpc`
//! feature (which pulls in `opentelemetry-proto` + `tonic`).

pub mod error;
pub mod exporter;
pub mod model;

#[cfg(feature = "grpc")]
pub mod grpc;

pub use error::OtlpError;
pub use exporter::{Exporter, ExporterConfig, Transport};
pub use model::{
    records_to_payload, record_to_log_record, value_to_any, AnyValue, KeyValue, LogRecord,
    LogsPayload, Resource, ResourceLogs, Scope, ScopeLogs,
};
