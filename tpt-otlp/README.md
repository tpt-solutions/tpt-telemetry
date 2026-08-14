# tpt-otlp

Typed-log → OpenTelemetry Protocol (OTLP) log-record mapping and exporters, for
[`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Provides an internal data model mirroring the OTLP/JSON logs schema, a typed
`Record` → OTLP converter, and an `Exporter` with config-driven transport
selection (`HTTP` or `gRPC`), batching, and retry/backoff. The HTTP/JSON path works
out of the box; the gRPC path is enabled by the `grpc` feature.

## Features

- **OTLP data model** — `Resource`, `ResourceLogs`, `Scope`, `ScopeLogs`,
  `LogRecord`, `AnyValue`, `KeyValue`, `LogsPayload` mirroring the OTLP/JSON schema.
- **Record → OTLP conversion** — `record_to_log_record` and `records_to_payload`
  map typed `Record`s (with `Value::Enum`, `Value::Str`, timestamps, etc.) into
  OTLP log records.
- **Dual transport**
  - **HTTP/JSON** (`Transport::Http`) — POSTs to `<endpoint>/v1/logs`. Default, no
    extra features required.
  - **gRPC** (`Transport::Grpc`) — OTLP/gRPC (port 4317) via `opentelemetry-proto`
    + `tonic`. Enabled by the `grpc` feature; TLS via the `tls` feature.
- **Batching** — records are split into request-sized batches automatically.
- **Retry / backoff** — exponential backoff with a bounded number of retries on
  transient transport failures.
- **Secret-safe** — `ExporterConfig`'s `Debug` redacts header values; exporting
  secret headers over a plaintext `http://` endpoint is a hard error when
  `require_tls` is set.

## Installation

```toml
[dependencies]
tpt-otlp = "0.1.0"

# For gRPC export:
tpt-otlp = { version = "0.1.0", features = ["grpc"] }

# For gRPC over TLS (rustls + system roots):
tpt-otlp = { version = "0.1.0", features = ["tls"] }
```

## Usage

### HTTP/JSON export (default)

```rust
use std::collections::HashMap;
use tpt_otlp::{Exporter, ExporterConfig, Transport};

let config = ExporterConfig {
    transport: Transport::Http,
    endpoint: "http://localhost:4318".into(),
    headers: HashMap::new(),
    batch_size: 1024,
    ..Default::default()
};
let exporter = Exporter::new(config);
// exporter.export(&[record])?;
```

### gRPC export

```toml
# Cargo.toml
[dependencies]
tpt-otlp = { version = "0.1.0", features = ["grpc"] }
```

```rust
use tpt_otlp::{Exporter, ExporterConfig, Transport};

let config = ExporterConfig {
    transport: Transport::Grpc,
    endpoint: "https://localhost:4317".into(),
    ..Default::default()
};
let exporter = Exporter::new(config);
```

### Building a payload manually

```rust
use tpt_otlp::{records_to_payload, record_to_log_record, value_to_any, LogRecord};

let payload = records_to_payload(&[record], "tpt-telemetry");
let json = serde_json::to_string_pretty(&payload).unwrap();
```

## Configuration reference

| Field | Default | Meaning |
|-------|---------|---------|
| `transport` | `Http` | `Http` (4318) or `Grpc` (4317, needs `grpc`) |
| `endpoint` | `http://localhost:4318` | OTLP collector base URL |
| `headers` | `{}` | extra HTTP/gRPC headers (treated as secrets) |
| `batch_size` | `1024` | max records per request |
| `timeout_ms` | `10000` | per-attempt request timeout |
| `max_retries` | `3` | retry attempts on transient failure |
| `base_backoff_ms` | `100` | base backoff (doubled each retry) |
| `scope_name` | `tpt-telemetry` | OTLP scope name |
| `require_tls` | `false` | error if secret headers sent over `http://` |

## API overview

- `Exporter::new(ExporterConfig)`, `Exporter::export(&[Record]) -> Result<(), OtlpError>`.
- `ExporterConfig` / `Transport` — configuration (HTTP or gRPC).
- `model::records_to_payload`, `record_to_log_record`, `value_to_any`,
  `AnyValue`, `KeyValue`, `LogRecord`, `LogsPayload`, `Resource`, `ResourceLogs`,
  `Scope`, `ScopeLogs`.
- `error::OtlpError` — serialize / transport / retry / insecure-transport / timestamp errors.
- `grpc` module — available under the `grpc` feature.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
