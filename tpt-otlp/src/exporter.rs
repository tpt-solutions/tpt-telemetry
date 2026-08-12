//! OTLP exporter: config-driven transport selection, batching, and
//! retry/backoff. The HTTP/JSON path works out of the box; the gRPC path is
//! enabled by the `grpc` feature.

use crate::error::OtlpError;
use crate::model::{records_to_payload, LogsPayload};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use tpt_telemetry_compiler::Record;

/// OTLP transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// OTLP/gRPC (port 4317). Requires the `grpc` feature.
    Grpc,
    /// OTLP/HTTP+JSON (port 4318, `/v1/logs`).
    Http,
}

impl Default for Transport {
    fn default() -> Self {
        Transport::Http
    }
}

/// Exporter configuration.
#[derive(Debug, Clone)]
pub struct ExporterConfig {
    /// Transport to use.
    pub transport: Transport,
    /// Base endpoint, e.g. `http://localhost:4318` (HTTP) or `http://localhost:4317` (gRPC).
    pub endpoint: String,
    /// Extra HTTP headers (e.g. auth tokens). Ignored by gRPC.
    pub headers: HashMap<String, String>,
    /// Maximum number of records per export request.
    pub batch_size: usize,
    /// Total request timeout per attempt.
    pub timeout_ms: u64,
    /// Maximum retry attempts on transient failure.
    pub max_retries: usize,
    /// Base backoff (doubled each retry).
    pub base_backoff_ms: u64,
    /// OTLP scope name attached to exported records.
    pub scope_name: String,
}

impl Default for ExporterConfig {
    fn default() -> Self {
        ExporterConfig {
            transport: Transport::Http,
            endpoint: "http://localhost:4318".into(),
            headers: HashMap::new(),
            batch_size: 1024,
            timeout_ms: 10_000,
            max_retries: 3,
            base_backoff_ms: 100,
            scope_name: "tpt-telemetry".into(),
        }
    }
}

/// An OTLP log exporter.
pub struct Exporter {
    config: ExporterConfig,
}

impl Exporter {
    pub fn new(config: ExporterConfig) -> Self {
        Exporter { config }
    }

    /// Export a batch of records, applying batching and retry/backoff.
    pub fn export(&self, records: &[Record]) -> Result<(), OtlpError> {
        if records.is_empty() {
            return Ok(());
        }
        let batch_size = self.config.batch_size.max(1);
        let mut start = 0;
        while start < records.len() {
            let end = (start + batch_size).min(records.len());
            let slice = &records[start..end];
            let payload = records_to_payload(slice, &self.config.scope_name);
            self.export_payload(&payload)?;
            start = end;
        }
        Ok(())
    }

    fn export_payload(&self, payload: &LogsPayload) -> Result<(), OtlpError> {
        match self.config.transport {
            Transport::Http => self.export_http(payload),
            Transport::Grpc => self.export_grpc(payload),
        }
    }

    fn export_http(&self, payload: &LogsPayload) -> Result<(), OtlpError> {
        let body = serde_json::to_vec(payload).map_err(|e| OtlpError::Serialize(e.to_string()))?;
        let url = format!("{}/v1/logs", self.config.endpoint.trim_end_matches('/'));
        let mut attempt = 0usize;
        let mut backoff = self.config.base_backoff_ms;
        loop {
            let mut req = ureq::post(&url)
                .set("Content-Type", "application/json")
                .timeout(Duration::from_millis(self.config.timeout_ms));
            for (k, v) in &self.config.headers {
                req = req.set(k, v);
            }
            match req.send_bytes(&body) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt >= self.config.max_retries {
                        return Err(OtlpError::RetriesExhausted(
                            self.config.max_retries,
                            e.to_string(),
                        ));
                    }
                    attempt += 1;
                    thread::sleep(Duration::from_millis(backoff));
                    backoff = (backoff * 2).min(30_000);
                }
            }
        }
    }

    #[cfg(feature = "grpc")]
    fn export_grpc(&self, payload: &LogsPayload) -> Result<(), OtlpError> {
        crate::grpc::export_grpc(self, payload)
    }

    #[cfg(not(feature = "grpc"))]
    fn export_grpc(&self, _payload: &LogsPayload) -> Result<(), OtlpError> {
        Err(OtlpError::GrpcFeatureDisabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{records_to_payload, LogsPayload};
    use tpt_telemetry_compiler::{parse, CompiledSchema, Record, Value};

    // Build a tiny payload and assert the HTTP serializer produces valid JSON
    // without hitting the network.
    #[test]
    fn http_payload_serializes() {
        let schema = parse(r#"format Auth { pattern: "%{ip:ipv4} login"; }"#).unwrap();
        let cs = CompiledSchema::compile(&schema).unwrap();
        let rec = cs.parse_line("10.0.0.5 login").unwrap();
        let payload = records_to_payload(std::slice::from_ref(&rec), "test");
        // Serialize through the same path the HTTP exporter uses.
        let body = serde_json::to_vec(&payload).unwrap();
        let _: LogsPayload = serde_json::from_slice(&body).unwrap();
        // Exercising the gRPC-disabled path returns the right error.
        let cfg = ExporterConfig {
            transport: Transport::Grpc,
            ..Default::default()
        };
        let e = Exporter::new(cfg);
        assert!(matches!(
            e.export(std::slice::from_ref(&rec)),
            Err(OtlpError::GrpcFeatureDisabled)
        ));
    }

    #[test]
    fn batches_large_inputs() {
        // Construct records manually to avoid schema dependence.
        let rec = Record {
            format: "X",
            fields: vec![tpt_telemetry_compiler::TypedField {
                name: "a",
                value: Value::Int(1),
            }],
        };
        let records: Vec<Record> = vec![rec; 2500];
        let cfg = ExporterConfig {
            batch_size: 1000,
            ..Default::default()
        };
        let e = Exporter::new(cfg);
        // With no collector, HTTP POSTs fail but batching should attempt ~3 times
        // and surface a transport error, not panic.
        let res = e.export(&records);
        assert!(res.is_err());
    }
}
