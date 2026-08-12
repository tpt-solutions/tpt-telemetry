//! OTLP log data model (JSON representation) and typed-log → OTLP mapping.
//!
//! The structures mirror the OpenTelemetry OTLP/JSON logs schema. We serialize to
//! JSON for the HTTP/JSON transport; the gRPC transport (behind the `grpc`
//! feature) converts the same model into protobuf via `opentelemetry-proto`.

use serde::Serialize;
use tpt_telemetry_compiler::{Record, Value};

/// Top-level OTLP logs payload.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LogsPayload {
    pub resource_logs: Vec<ResourceLogs>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceLogs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<Resource>,
    pub scope_logs: Vec<ScopeLogs>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Resource {
    pub attributes: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopeLogs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    pub log_records: Vec<LogRecord>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Scope {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogRecord {
    /// Nanoseconds since the Unix epoch, as a string (OTLP/JSON uses string).
    pub time_unix_nano: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_time_unix_nano: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<AnyValue>,
    pub attributes: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyValue {
    pub key: String,
    pub value: AnyValue,
}

/// An OTLP `AnyValue` (one-of). Only the populated variant is serialized.
#[derive(Debug, Clone, Serialize, Default)]
pub struct AnyValue {
    #[serde(rename = "stringValue", skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(rename = "boolValue", skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(rename = "intValue", skip_serializing_if = "Option::is_none")]
    pub int_value: Option<i64>,
    #[serde(rename = "doubleValue", skip_serializing_if = "Option::is_none")]
    pub double_value: Option<f64>,
}

impl AnyValue {
    pub fn string(s: impl Into<String>) -> Self {
        AnyValue {
            string_value: Some(s.into()),
            ..Default::default()
        }
    }
    pub fn int(v: i64) -> Self {
        AnyValue {
            int_value: Some(v),
            ..Default::default()
        }
    }
    pub fn bool(v: bool) -> Self {
        AnyValue {
            bool_value: Some(v),
            ..Default::default()
        }
    }
    pub fn double(v: f64) -> Self {
        AnyValue {
            double_value: Some(v),
            ..Default::default()
        }
    }
}

/// Map a compiler [`Value`] into an OTLP [`AnyValue`].
pub fn value_to_any(v: &Value) -> AnyValue {
    match v {
        Value::Str(s) => AnyValue::string(*s),
        Value::OwnedString(s) => AnyValue::string(s.clone()),
        Value::Int(i) => AnyValue::int(*i),
        Value::Uint(u) => AnyValue::int(*u as i64),
        Value::Float(f) => AnyValue::double(*f),
        Value::Bool(b) => AnyValue::bool(*b),
        Value::Enum(idx) => AnyValue::int(*idx as i64),
        Value::Timestamp(ts) => AnyValue::int(*ts),
        Value::Ip(ip) => AnyValue::string(ip.to_string()),
        Value::Ipv4(a) => AnyValue::string(a.to_string()),
        Value::Ipv6(a) => AnyValue::string(a.to_string()),
        Value::Mac(m) => AnyValue::string(format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            m[0], m[1], m[2], m[3], m[4], m[5]
        )),
    }
}

/// Severity numbers (OTLP severity table subset).
pub const SEVERITY_INFO: u32 = 9;
pub const SEVERITY_WARN: u32 = 13;
pub const SEVERITY_ERROR: u32 = 17;

/// Build an OTLP [`LogRecord`] from a typed [`Record`].
///
/// The body is the `message` field if present, otherwise the format name. A
/// numeric/enum `severity` field is mapped to an OTLP severity number.
pub fn record_to_log_record(rec: &Record) -> LogRecord {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    let body = rec
        .fields
        .iter()
        .find(|f| f.name == "message")
        .map(|f| value_to_any(&f.value))
        .unwrap_or_else(|| AnyValue::string(rec.format));

    let severity = rec
        .fields
        .iter()
        .find(|f| f.name == "severity")
        .map(|f| match &f.value {
            Value::Enum(idx) => match idx {
                0..=1 => 17u32, // EMERGENCY/ALERT
                2..=3 => 17,     // CRITICAL/ERROR
                4 => 13,        // WARNING
                5 => 11,        // NOTICE
                6 => 9,         // INFO
                _ => 7,         // DEBUG
            },
            Value::Int(i) => match i {
                0..=3 => 17,
                4 => 13,
                5 | 6 => 9,
                _ => 7,
            },
            _ => SEVERITY_INFO,
        })
        .unwrap_or(SEVERITY_INFO);

    let attributes = rec
        .fields
        .iter()
        .map(|f| KeyValue {
            key: f.name.to_string(),
            value: value_to_any(&f.value),
        })
        .collect();

    LogRecord {
        time_unix_nano: now.to_string(),
        observed_time_unix_nano: None,
        severity_number: Some(severity),
        severity_text: None,
        body: Some(body),
        attributes,
    }
}

/// Build a full [`LogsPayload`] from records, tagging them with a scope name.
pub fn records_to_payload(records: &[Record], scope_name: &str) -> LogsPayload {
    let log_records: Vec<LogRecord> = records.iter().map(record_to_log_record).collect();
    if log_records.is_empty() {
        return LogsPayload::default();
    }
    LogsPayload {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".into(),
                    value: AnyValue::string("tpt-telemetry"),
                }],
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(Scope {
                    name: scope_name.to_string(),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                }),
                log_records,
            }],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_telemetry_compiler::{CompiledSchema, Value};
    use tpt_telemetry_schema::parse;

    #[test]
    fn maps_record_to_otlp() {
        let schema = parse(
            r#"format CiscoASA { pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}"; coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG }; }"#,
        )
        .unwrap();
        let cs = CompiledSchema::compile(&schema).unwrap();
        let rec = cs
            .parse_line("%ASA-6-302013: Built inbound TCP connection")
            .unwrap();
        let payload = records_to_payload(std::slice::from_ref(&rec), "test");
        assert_eq!(payload.resource_logs.len(), 1);
        let lr = &payload.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(lr.severity_number, Some(SEVERITY_INFO));
        // Check serialization is valid JSON and round-trips.
        let json = serde_json::to_string(&payload).unwrap();
        let back: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(back["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["severityNumber"]
            .as_u64()
            .is_some());
    }

    #[test]
    fn any_value_only_populates_one_variant() {
        let v = AnyValue::int(42);
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, r#"{"intValue":42}"#);
    }
}
