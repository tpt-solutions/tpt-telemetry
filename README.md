# tpt-telemetry

A strongly-typed observability contract and legacy log parser for Syslog, CEF, and
LEEF telemetry. Security devices and network gear emit unstructured Syslog, CEF, or
LEEF; `tpt-telemetry` turns those messy legacy logs into strongly-typed Rust structs
via a schema language (`.tpt-log`) and AI-assisted inference.

## Features

- **Schema-Driven Parsing** — Define log formats in a `.tpt-log` schema; the compiler
  generates zero-allocation parsers.
- **Grok-Compatible** — Import and execute standard Elastic Common Schema (ECS) Grok
  patterns for easy migration from Logstash/Elasticsearch.
- **Streaming Architecture** — Parse multi-gigabyte log streams with zero heap
  allocations per line.
- **OTLP Native** — Translate parsed, typed logs into OpenTelemetry Protocol (OTLP)
  payloads.

## Workspace layout

| Crate | Purpose |
|-------|---------|
| `tpt-telemetry-schema` | DSL for defining log formats, field extractions, and type coercions. |
| `tpt-grok-engine` | SIMD-accelerated Grok pattern matcher. |
| `tpt-telemetry-compiler` | `build.rs` codegen from `.tpt-log` schemas. |
| `tpt-telemetry-core` | Parsing runtime and streaming line/frame reader. |
| `tpt-syslog-server` | High-throughput UDP/TCP receiver (RFC3164 / RFC5424). |
| `tpt-inference` | LLM-assisted `.tpt-log` schema suggestion. |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Copyright TPT Solutions.
