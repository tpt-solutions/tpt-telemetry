# tpt-daemon

Unified `tpt-telemetry` daemon: syslog ingest → parse → OTLP export, for
[`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Wires the syslog receiver (`tpt-syslog-server`), the schema-driven parser
(`tpt-telemetry-core`) and the OTLP exporter (`tpt-otlp`) into a single
long-running process with a Prometheus metrics endpoint and health check. Ships as
both a library (`tpt_daemon`) and a binary (`tpt-daemon`).

## Features

- **Full pipeline in one process** — binds syslog UDP/TCP receivers, compiles a
  `.tpt-log` schema, parses each message, and exports typed `Record`s to OTLP.
- **Backpressure-aware worker** — drains the syslog ring buffer; the exporter
  performs internal batching, retry, and backoff.
- **Prometheus metrics** — `/metrics` endpoint exposing `received`, `exported`,
  `errors`, plus syslog stats; `/healthz` health check.
- **TOML config with secret interpolation** — `${ENV_VAR}` references in paths,
  binds, the OTLP endpoint, header values, and the log level are resolved from the
  process environment so secrets stay out of the config file.
- **Graceful shutdown** — Ctrl-C handler and `Drop` based shutdown; worker and
  metrics threads join cleanly.
- **gRPC export** — built with the `tpt-otlp` `grpc` feature enabled.

## Installation / build

```bash
cargo build --release -p tpt-daemon
# binary: target/release/tpt-daemon
```

## Usage

```bash
# default config path: tpt-daemon.toml
tpt-daemon --config /etc/tpt-telemetry/tpt-daemon.toml
```

### Example configuration

```toml
schema.path = "schemas/cisco_asa.tpt-log"

[syslog.udp]
bind = "0.0.0.0:514"
[syslog.tcp]
bind = "0.0.0.0:601"
framing = "auto"
max_frame_len = 1048576
max_connections = 1024
ring_capacity = 65536
read_timeout_ms = 250

[otlp]
endpoint = "https://collector.example:4317"
transport = "grpc"
batch_size = 1024
timeout_ms = 10000
max_retries = 3
base_backoff_ms = 100
scope_name = "tpt-daemon"
require_tls = true
headers = { Authorization = "Bearer ${OTLP_TOKEN}" }

[metrics]
bind = "0.0.0.0:9102"

[logging]
level = "info"
```

See `packaging/tpt-daemon.example.toml` and `packaging/tpt-daemon.service` for a
systemd deployment template.

## Library usage

```rust
use tpt_daemon::{Daemon, DaemonConfig};
use std::sync::atomic::Ordering;

let mut cfg: DaemonConfig = toml::from_str(CONFIG).unwrap();
cfg.interpolate_env();

let daemon = Daemon::new(cfg).unwrap();
let running = daemon.start();
println!("udp={} tcp={} metrics={}", running.udp, running.tcp, running.metrics);
running.stop(); // or rely on Drop
```

## Configuration reference

| Section | Keys |
|---------|------|
| `schema` | `path` — `.tpt-log` file |
| `syslog.udp` / `syslog.tcp` | `bind`, `framing` (`auto`/`octet`/`lf`), `max_frame_len`, `max_connections` |
| `syslog` | `ring_capacity`, `read_timeout_ms` |
| `otlp` | `endpoint`, `transport` (`http`/`grpc`), `batch_size`, `timeout_ms`, `max_retries`, `base_backoff_ms`, `scope_name`, `require_tls`, `headers` |
| `metrics` | `bind` |
| `logging` | `level` |

## API overview

- `Daemon::new(DaemonConfig) -> Result<Self>` — bind listeners, compile schema,
  build exporter.
- `Daemon::start() -> RunningDaemon` — spawn worker + metrics threads.
- `RunningDaemon` — `udp`, `tcp`, `metrics` addresses; `stop()`.
- `Daemon::local_udp_addr()`, `local_tcp_addr()`, `local_metrics_addr()`,
  `stop_flag()`.
- `DaemonConfig` + sub-configs — TOML-deserialized, with `interpolate_env()`.
- `config`, `metrics_http` modules.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
