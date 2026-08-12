# tpt-telemetry — Project Todo

License: MIT OR Apache-2.0 · TPT Solutions

---

## Phase 0 — Repository & Licensing Setup
- [ ] Initialize cargo workspace (`Cargo.toml` with `[workspace]` members)
- [ ] Add `LICENSE-MIT` and `LICENSE-APACHE` files (dual MIT OR Apache-2.0), copyright TPT Solutions
- [ ] Add root `README.md`, `.gitignore`, `rust-toolchain.toml`
- [ ] Initialize git repo, initial commit
- [ ] Set up `CONTRIBUTING.md` and issue/PR templates

## Phase 1 — Schema DSL (`tpt-telemetry-schema`)
- [ ] Define `.tpt-log` grammar (formats, `pattern`, `extract ... using regex`, `coerce ... to enum`, type coercions)
- [ ] Choose/implement parser for the DSL (e.g. `pest` or hand-written)
- [ ] Define AST types for schema (formats, fields, extractions, coercions, PII annotations)
- [ ] Support Grok-pattern compatibility layer (import standard Grok patterns, incl. ECS patterns)
- [ ] Unit tests for DSL parsing edge cases

## Phase 2 — Grok Engine (`tpt-grok-engine`)
- [ ] Implement baseline (non-SIMD) Grok pattern matcher for correctness
- [ ] Add SIMD-accelerated matching (e.g. via `memchr`/portable-simd) for hot paths
- [ ] Compatibility test suite against standard Elastic Common Schema (ECS) Grok patterns
- [ ] Benchmarks comparing baseline vs SIMD paths

## Phase 3 — Schema Compiler (`tpt-telemetry-compiler`, build.rs codegen)
- [ ] Design codegen strategy: schema AST → generated Rust parser source
- [ ] Implement `build.rs` integration point consuming `.tpt-log` files
- [ ] Generate zero-copy, zero-allocation parser functions per schema format
- [ ] Generate typed Rust structs matching schema fields (with enum coercions)
- [ ] Golden-file tests comparing generated code across schema changes
- [ ] Error reporting for invalid schemas (compile-time diagnostics)

## Phase 4 — Core Parsing Runtime (`tpt-telemetry-core`)
- [ ] Define public API: feed raw log lines/bytes → typed structs
- [ ] Wire generated parsers (from Phase 3) into runtime dispatch
- [ ] Streaming line/frame reader for multi-gigabyte inputs (chunked, zero-copy)
- [ ] Verify zero heap allocations in steady-state parse loop (allocation-tracking test harness, e.g. custom `GlobalAlloc` counter)

## Phase 5 — Syslog Server (`tpt-syslog-server`)
- [ ] UDP receiver (RFC3164 + RFC5424 framing)
- [ ] TCP receiver (RFC5424 octet-counting / non-transparent framing)
- [ ] Kernel-level `SO_RXQ_OVFL` drop-counter integration (Linux)
- [ ] Ring-buffer backpressure mechanism to bound memory under log floods
- [ ] Integration tests: high-throughput send/receive, overflow/backpressure behavior
- [ ] Graceful shutdown / connection lifecycle handling

## Phase 6 — Security & Safety
- [ ] Log injection prevention: sanitize extracted fields before downstream SIEM query rendering
- [ ] PII redaction: schema-level annotations to hash/mask emails, IPs, credit card numbers
- [ ] Security-focused test suite (injection payloads, redaction correctness)
- [ ] Dependency audit (`cargo audit`) baseline

## Phase 7 — OTLP Export
- [ ] Define internal typed-log → OTLP log record mapping
- [ ] Implement OTLP/gRPC exporter (tonic-based)
- [ ] Implement OTLP/HTTP+protobuf exporter
- [ ] Config-driven transport selection (gRPC vs HTTP)
- [ ] Batching/retry/backoff for exporters
- [ ] Integration tests against a local OTLP collector (e.g. otel-collector in CI)

## Phase 8 — Performance Validation
- [ ] Criterion benchmark suite across grok engine, compiler-generated parsers, and end-to-end pipeline
- [ ] Allocation-tracking CI gate asserting zero heap allocations in steady-state loop
- [ ] Fuzz testing (e.g. `cargo-fuzz`) for schema parser, Grok engine, and syslog framing
- [ ] Load test harness validating 1M lines/sec/core target on reference hardware
- [ ] Document benchmark methodology and results (perf report)

## Phase 9 — LLM-Assisted Inference (`tpt-inference`)
- [ ] Define inference-provider trait/interface (sample logs in → suggested `.tpt-log` schema out)
- [ ] Implement Claude API integration for schema suggestion from raw log samples
- [ ] Prompt design + validation loop (suggested schema must compile via Phase 3 compiler)
- [ ] CLI or library entry point for `tpt-inference`
- [ ] Tests with representative log samples (Cisco ASA, generic CEF/LEEF, RFC5424)

## Phase 10 — Documentation & Examples
- [ ] Top-level architecture doc (mirrors spec.txt's component diagram)
- [ ] `.tpt-log` schema authoring guide + Grok pattern migration guide (from Logstash/ES)
- [ ] Example schemas (Cisco ASA, generic CEF, LEEF) with sample logs
- [ ] End-to-end example: syslog server → parser → OTLP export
- [ ] API docs (`cargo doc`) polish pass per crate

## Phase 11 — CI/CD & Release
- [ ] CI pipeline: build, test, lint (`clippy`, `fmt`), audit, fuzz smoke tests
- [ ] Cross-platform build matrix (Linux primary for `SO_RXQ_OVFL`; document platform gaps)
- [ ] Versioning strategy across workspace crates (independent vs lockstep)
- [ ] crates.io metadata (description, keywords, categories, license fields) per crate
- [ ] Publish workflow (`cargo publish` order respecting inter-crate dependencies)
- [ ] Tag/release process + CHANGELOG conventions
