# tpt-telemetry Architecture

This document mirrors the component diagram in `spec.txt` and describes how the
crates fit together.

## Data flow

```
                 .tpt-log schema
                        │
                        ▼
         ┌──────────────────────────────┐
         │   tpt-telemetry-compiler     │  AST → CompiledSchema (flat segments)
         │  • zero-copy matcher         │  • Rust codegen (golden files)
         │  • Rust codegen              │
         └───────────────┬──────────────┘
                         │ CompiledSchema
                         ▼
         ┌──────────────────────────────┐
         │     tpt-telemetry-core       │  Parser dispatch + StreamReader
         │  • feed raw line/bytes       │  • zero-alloc match hot path
         │  • typed Record (coerce+     │  • allocation-tracking harness
         │    redact)                   │
         └───────────────┬──────────────┘
                         │ Record
                         ▼
                  OTLP export  (tpt-otlp)
                         │ OTLP/logs (HTTP+JSON or gRPC)
                         ▼
   tpt-daemon  (syslog UDP/TCP ingest → parse → OTLP)
```

## Crate responsibilities

- **`tpt-telemetry-schema`** — Parses the `.tpt-log` DSL into an AST
  (`Schema`/`Format`/`Pattern`/`Extract`/`Coerce`/`Redact`). Ships the standard
  Grok pattern library (Logstash base + ECS subset) so schemas can reference
  `%{IP:client}` exactly as in a Logstash pipeline.
- **`tpt-grok-engine`** — A SIMD-accelerated Grok matcher (`memchr` fast-scan
  before `regex`). Used for arbitrary Grok patterns and ECS compatibility; the
  compiler's zero-copy matcher handles the common native-typed subset without
  `regex` allocation.
- **`tpt-telemetry-compiler`** — Lowers the AST into a `CompiledSchema`: a flat
  list of `Seg`ments (literals + typed captures). At run time the matcher borrows
  field values directly from the input line. Also emits Rust source for
  golden-file testing and `build.rs` integration.
- **`tpt-telemetry-core`** — The public `Parser` API over a `CompiledSchema`,
  plus a chunked, allocation-reusing `StreamReader` for multi-gigabyte inputs.
  Provides an opt-in allocation-tracking gate (`alloc-counter` feature).
- **`tpt-syslog-server`** — UDP/TCP receiver (RFC3164 / RFC5424) with a bounded
  ring buffer for backpressure, Linux `SO_RXQ_OVFL` overflow accounting, and
  per-frame / per-connection caps. Feeds `Message`s into the parser.
- **`tpt-otlp`** — Internal typed-log → OTLP log-record mapping plus HTTP/JSON
  and gRPC (tonic, `grpc`/`tls` features) exporters with config-driven transport
  selection, batching, retry, and backoff.
- **`tpt-inference`** — LLM-assisted schema suggestion from raw samples (Claude /
  OpenAI / OpenRouter / Grok / Ollama) behind a provider trait with a
  validate-and-retry loop.
- **`tpt-daemon`** — Unified binary that wires the above together: binds the
  syslog receivers, compiles the schema, parses each message, and exports the
  typed records to OTLP. Ships a Prometheus `/metrics` + `/healthz` endpoint and
  a TOML config with `${ENV_VAR}` secret interpolation.

## Zero-copy / zero-allocation design

The compiler represents every pattern as `Seg::Literal` / `Seg::Capture`. The
matcher (`match_segments`) walks the segments by reference: captured field
values are `&str` slices into the input line, and numeric / timestamp / enum
coercions are parsed in place. The only heap use in the steady-state loop is the
small, caller-reused `MatchCtx` (`Vec` cleared each call, never reallocated once
warmed up). The `alloc-counter` test asserts that the framing + matching hot path
performs no new allocations.

Typed `Record`s allocate a capacity-stable `Vec` of fields; redaction (`mask` /
`hash`) produces `OwnedString` values by design, since it must transform the
captured text.
