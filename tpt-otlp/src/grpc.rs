//! gRPC OTLP export (OpenTelemetry `opentelemetry-proto` + `tonic`), enabled by
//! the `grpc` feature. Converts the in-memory `LogsPayload` model into protobuf
//! `ExportLogsServiceRequest` and sends it over a gRPC channel.
//!
//! When compiled with the `tls` feature, `https://` endpoints are upgraded to a
//! TLS channel (using the system root store). `ExporterConfig::headers` are
//! forwarded as gRPC metadata (the HTTP path previously ignored them).

use crate::error::OtlpError;
use crate::exporter::Exporter;
use crate::model::{
    AnyValue as ModelAny, KeyValue as ModelKv, LogRecord as ModelLr, LogsPayload,
    ResourceLogs as ModelRl, ScopeLogs as ModelSl,
};

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{
    AnyValue as PlAny, InstrumentationScope, KeyValue as PlKv,
};
use opentelemetry_proto::tonic::logs::v1::{
    LogRecord as PlLr, ResourceLogs as PlRl, ScopeLogs as PlSl,
};
use opentelemetry_proto::tonic::resource::v1::Resource as PlResource;
use tonic::transport::Channel;

fn any_to_proto(a: &ModelAny) -> PlAny {
    use opentelemetry_proto::tonic::common::v1::any_value::Value as PlAnyValue;
    let v = if let Some(s) = &a.string_value {
        PlAnyValue::StringValue(s.clone())
    } else if let Some(i) = a.int_value {
        PlAnyValue::IntValue(i)
    } else if let Some(b) = a.bool_value {
        PlAnyValue::BoolValue(b)
    } else if let Some(d) = a.double_value {
        PlAnyValue::DoubleValue(d)
    } else {
        PlAnyValue::StringValue(String::new())
    };
    PlAny { value: Some(v) }
}

fn kv_to_proto(kv: &ModelKv) -> PlKv {
    PlKv {
        key: kv.key.clone(),
        value: Some(any_to_proto(&kv.value)),
    }
}

fn lr_to_proto(lr: &ModelLr) -> Result<PlLr, OtlpError> {
    // A malformed timestamp must surface an error rather than silently
    // collapsing to the Unix epoch (which would corrupt log ordering).
    let time_unix_nano = lr
        .time_unix_nano
        .parse::<u64>()
        .map_err(|_| OtlpError::InvalidTimestamp(lr.time_unix_nano.clone()))?;
    let observed_time_unix_nano = match lr.observed_time_unix_nano.as_deref() {
        Some(s) if !s.is_empty() => Some(
            s.parse::<u64>()
                .map_err(|_| OtlpError::InvalidTimestamp(s.to_string()))?,
        ),
        _ => None,
    };
    Ok(PlLr {
        time_unix_nano,
        observed_time_unix_nano: observed_time_unix_nano.unwrap_or(0),
        severity_number: lr.severity_number.unwrap_or(0) as i32,
        severity_text: lr.severity_text.clone().unwrap_or_default(),
        body: lr.body.as_ref().map(any_to_proto),
        attributes: lr.attributes.iter().map(kv_to_proto).collect(),
        ..Default::default()
    })
}

fn payload_to_proto(p: &LogsPayload) -> Result<ExportLogsServiceRequest, OtlpError> {
    let resource_logs = p
        .resource_logs
        .iter()
        .map(|rl: &ModelRl| {
            Ok(PlRl {
                resource: rl.resource.as_ref().map(|r| PlResource {
                    attributes: r.attributes.iter().map(kv_to_proto).collect(),
                    ..Default::default()
                }),
                scope_logs: rl
                    .scope_logs
                    .iter()
                    .map(|sl: &ModelSl| {
                        Ok(PlSl {
                            scope: sl.scope.as_ref().map(|s| InstrumentationScope {
                                name: s.name.clone(),
                                version: s.version.clone().unwrap_or_default(),
                                ..Default::default()
                            }),
                            log_records: sl
                                .log_records
                                .iter()
                                .map(lr_to_proto)
                                .collect::<Result<Vec<_>, _>>()?,
                            ..Default::default()
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                ..Default::default()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExportLogsServiceRequest { resource_logs })
}

/// Export a payload over gRPC. Connects to the configured endpoint.
pub fn export_grpc(exporter: &Exporter, payload: &LogsPayload) -> Result<(), OtlpError> {
    let rt = tokio_runtime()?;
    rt.block_on(async {
        let endpoint = exporter.config.endpoint.trim_end_matches('/').to_string();
        let channel = make_channel(&endpoint).await?;
        let mut client = LogsServiceClient::new(channel);
        let req = payload_to_proto(payload)?;

        let mut grpc_req = tonic::Request::new(req);
        for (k, v) in &exporter.config.headers {
            use std::str::FromStr;
            let mv = tonic::metadata::MetadataValue::from_str(v.as_str()).map_err(|e| {
                OtlpError::Transport(format!("invalid gRPC header value for `{k}`: {e}"))
            })?;
            let key = tonic::metadata::MetadataKey::from_bytes(k.as_bytes()).map_err(|e| {
                OtlpError::Transport(format!("invalid gRPC header name `{k}`: {e}"))
            })?;
            grpc_req.metadata_mut().insert(key, mv);
        }

        client
            .export(grpc_req)
            .await
            .map_err(|e| OtlpError::Transport(e.to_string()))?;
        Ok(())
    })
}

/// Build a gRPC channel, upgrading `https://` endpoints to TLS when the `tls`
/// feature is compiled in.
async fn make_channel(endpoint: &str) -> Result<Channel, OtlpError> {
    // Bound how long we wait for a connection so a missing collector fails
    // fast instead of hanging the exporter thread.
    let connect_timeout = std::time::Duration::from_secs(2);
    if endpoint.starts_with("https://") {
        #[cfg(feature = "tls")]
        {
            Channel::from_shared(endpoint.to_string())
                .map_err(|e| OtlpError::Transport(e.to_string()))?
                .tls_config(tonic::transport::ClientTlsConfig::new().with_native_roots())
                .map_err(|e| OtlpError::Transport(e.to_string()))?
                .connect_timeout(connect_timeout)
                .connect()
                .await
                .map_err(|e| OtlpError::Transport(e.to_string()))
        }
        #[cfg(not(feature = "tls"))]
        {
            Err(OtlpError::Transport(
                "https endpoint requires the `tls` feature".into(),
            ))
        }
    } else {
        Channel::from_shared(endpoint.to_string())
            .map_err(|e| OtlpError::Transport(e.to_string()))?
            .connect_timeout(connect_timeout)
            .connect()
            .await
            .map_err(|e| OtlpError::Transport(e.to_string()))
    }
}

/// A shared single-threaded tokio runtime for the gRPC path.
fn tokio_runtime() -> Result<tokio::runtime::Runtime, OtlpError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OtlpError::Transport(e.to_string()))
}
