# tpt-telemetry

[![CI](https://github.com/tpt-solutions/tpt-telemetry/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-telemetry/actions/workflows/ci.yml)

A strongly-typed observability contract and legacy log parser for Syslog, CEF, and
LEEF telemetry. Security devices and network gear emit unstructured Syslog, CEF, or
LEEF; `tpt-telemetry` turns those messy legacy logs into strongly-typed records via
a schema language (`.tpt-log`) and AI-assisted inference, then exports them as
OpenTelemetry (OTLP).

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the pipeline and crate design,
[`SCHEMA_GUIDE.md`](SCHEMA_GUIDE.md) for the `.tpt-log` DSL, and `spec.txt` for the
original design brief.

## Features

- **Schema-Driven Parsing** — Define log formats in a `.tpt-log` schema; the compiler
  generates zero-allocation parsers with native-typed captures.
- **Grok-Compatible** — Import and execute standard Elastic Common Schema (ECS) Grok
  patterns for easy migration from Logstash/Elasticsearch.
- **Streaming Architecture** — Parse multi-gigabyte log streams with zero heap
  allocations per line (see the `alloc-counter` test gate in `tpt-telemetry-core`).
- **OTLP Native** — Translate parsed, typed records into OpenTelemetry Protocol
  (OTLP) log payloads over HTTP/JSON or gRPC.
- **Syslog Ingest** — High-throughput UDP/TCP receiver (RFC3164 / RFC5424) with
  backpressure and overflow accounting.
- **AI-Assisted Schema Suggestion** — `tpt-inference` proposes `.tpt-log` schemas
  from raw samples behind a provider trait (Claude / OpenAI / OpenRouter / Grok /
  Ollama).

## Workspace layout

This is a Cargo workspace. Crates are listed in pipeline order:

| Crate | Purpose |
|-------|---------|
| `tpt-telemetry-schema` | `.tpt-log` DSL grammar/AST/parser + standard Grok library. |
| `tpt-grok-engine` | SIMD-accelerated Grok matcher (`memchr` fast-scan + `regex`). |
| `tpt-telemetry-compiler` | AST → `CompiledSchema` (flat `Seg`s) + Rust codegen. |
| `tpt-telemetry-core` | Public `Parser` API, `StreamReader`, allocation harness. |
| `tpt-syslog-server` | UDP/TCP syslog receiver (RFC3164 / RFC5424) + backpressure. |
| `tpt-inference` | LLM-assisted schema suggestion behind a provider trait. |
| `tpt-otlp` | Typed record → OTLP mapping + HTTP/JSON and gRPC exporters. |
| `tpt-daemon` | Unified binary wiring ingest → parse → OTLP (+ Prometheus metrics). |

Non-crate dirs: `examples/` (sample schemas/runs), `fuzz/`, `packaging/`, `target/`.

## Quick start

### 1. Build & test the workspace

```bash
cargo build --workspace
cargo test  --workspace
```

### 2. Define a schema

```tpt-log
// examples/schemas/cisco_asa.tpt-log
format CiscoASA {
  pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
  coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
  redact message with mask;
}
```

### 3. Run the daemon

Configure ingest + OTLP export in TOML (see
[`packaging/tpt-daemon.example.toml`](packaging/tpt-daemon.example.toml)), then run:

```bash
cargo run -p tpt-daemon -- --config packaging/tpt-daemon.example.toml
```

`tpt-daemon` binds the syslog receivers, compiles the schema, parses each message,
and exports typed records to OTLP. It also serves Prometheus `/metrics` and
`/healthz` endpoints and supports `${ENV_VAR}` secret interpolation. Useful
subcommands:

```bash
cargo run -p tpt-daemon -- --version          # print version
cargo run -p tpt-daemon -- --check --config packaging/tpt-daemon.example.toml  # validate config
cargo run -p tpt-daemon -- --healthcheck --config packaging/tpt-daemon.example.toml  # probe /healthz

# Send a sample Cisco ASA line at a running daemon (UDP 514 by default):
cargo run -p tpt-daemon --bin tpt-send-log -- --udp 127.0.0.1:514 \
    --message "%ASA-6-302013: Built inbound TCP connection"
```

A ready-made local stack (daemon + OpenTelemetry Collector + Prometheus) is available
via `docker compose up --build` — see [`docker-compose.yml`](docker-compose.yml).

## Build, test, lint

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets
cargo fmt   --all
```

- Allocation-free guarantee is gated: `cargo test -p tpt-telemetry-core --features alloc-counter`.
- Codegen has a golden file: `cargo run -p tpt-telemetry-compiler --example gen_golden`
  regenerates `tests/golden/cisco_asa.rs`. Review/confirm any diff.
- Toolchain is pinned in `rust-toolchain.toml` (Rust 1.85 / edition 2021).

## Documentation

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — crate responsibilities and zero-copy design.
- [`SCHEMA_GUIDE.md`](SCHEMA_GUIDE.md) — the `.tpt-log` DSL reference.
- [`PERFORMANCE.md`](PERFORMANCE.md) — benchmarks and allocation notes.
- [`PLATFORMS.md`](PLATFORMS.md) — supported targets.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — PR/commit guidance.
- [`SECURITY.md`](SECURITY.md) — vulnerability reporting.
- [`VERSIONING.md`](VERSIONING.md) — release/version policy.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Copyright TPT Solutions.
