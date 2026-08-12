# tpt-telemetry — Project Todo

License: MIT OR Apache-2.0 · TPT Solutions

> **Session progress (2026-08-12 → 2026-08-13):** Phases 0–4 implemented and tested; security redaction/sanitization (Phase 6) and example schemas/docs (Phase 10) landed as slices. Phases 5, 7, 9 were implemented in a prior pass (real code + passing tests) but left marked "deferred" in this list; they are now reconciled below. Phase 6 `cargo audit` baseline and Phase 10 `cargo doc` polish landed this session.
> - Phase 0: workspace + dual licenses + README + rust-toolchain + .gitignore + git init + CONTRIBUTING.md.
> - Phase 1: `tpt-telemetry-schema` — `.tpt-log` pest grammar, AST, parser, standard/ECS Grok pattern DB, unit tests.
> - Phase 2: `tpt-grok-engine` — Grok→regex compiler (recursive pattern expansion), baseline matcher, `memchr` SIMD fast-scan hot path, Criterion bench, tests.
> - Phase 3: `tpt-telemetry-compiler` — AST → `CompiledSchema` (flat zero-copy `Seg`ments), zero-alloc matcher (borrowed `&str` captures + typed coercions), Rust codegen + golden-file test, `build.rs` integration helper.
> - Phase 4: `tpt-telemetry-core` — `Parser` dispatch API, allocation-reusing `StreamReader`, opt-in `alloc-counter` zero-alloc gate (passing under parallel `cargo test`).
> - Phase 5: `tpt-syslog-server` — UDP/TCP receivers (RFC3164/RFC5424 framing), Linux `SO_RXQ_OVFL`, bounded ring-buffer backpressure, integration tests, graceful shutdown.
> - Phase 6 slice: redaction (`mask`/`hash`) + log-injection sanitization utility + tests; `cargo audit` baseline (0 vulns / 203 deps).
> - Phase 7: `tpt-otlp` — typed-log→OTLP model, HTTP/JSON + gRPC (tonic, feature-gated) exporters, config-driven transport, batching/retry/backoff.
> - Phase 9: `tpt-inference` — provider trait + validate-and-retry loop, Claude/OpenAI/OpenRouter/Grok/Ollama providers, CLI, tests.
> - Phase 10 slice: `ARCHITECTURE.md`, `SCHEMA_GUIDE.md`, example schemas (Cisco ASA / RFC5424 / CEF) + sample logs + end-to-end integration test; `cargo doc` polish (0 intra-doc warnings).
> - Phase 8: Performance Validation — Criterion suite (grok engine, compiler parsers, e2e + throughput), zero-alloc CI gate, cargo-fuzz targets + fuzz-smoke tests, load harness (~1.72M lines/sec/core, above the 1M target), and `PERFORMANCE.md` report.
> - Phases 11: not yet implemented (deferred — broad CI / release-publish work). Phase 7's live-collector integration test also deferred (needs external OTLP collector).

---

## Phase 0 — Repository & Licensing Setup
- [x] Initialize cargo workspace (`Cargo.toml` with `[workspace]` members)
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` files (dual MIT OR Apache-2.0), copyright TPT Solutions
- [x] Add root `README.md`, `.gitignore`, `rust-toolchain.toml`
- [x] Initialize git repo, initial commit
- [x] Set up `CONTRIBUTING.md` and issue/PR templates

## Phase 1 — Schema DSL (`tpt-telemetry-schema`)
- [x] Define `.tpt-log` grammar (formats, `pattern`, `extract ... using regex`, `coerce ... to enum`, type coercions)
- [x] Choose/implement parser for the DSL (e.g. `pest` or hand-written)
- [x] Define AST types for schema (formats, fields, extractions, coercions, PII annotations)
- [x] Support Grok-pattern compatibility layer (import standard Grok patterns, incl. ECS patterns)
- [x] Unit tests for DSL parsing edge cases

## Phase 2 — Grok Engine (`tpt-grok-engine`)
- [x] Implement baseline (non-SIMD) Grok pattern matcher for correctness
- [x] Add SIMD-accelerated matching (e.g. via `memchr`/portable-simd) for hot paths
- [x] Compatibility test suite against standard Elastic Common Schema (ECS) Grok patterns
- [x] Benchmarks comparing baseline vs SIMD paths

## Phase 3 — Schema Compiler (`tpt-telemetry-compiler`, build.rs codegen)
- [x] Design codegen strategy: schema AST → generated Rust parser source
- [x] Implement `build.rs` integration point consuming `.tpt-log` files
- [x] Generate zero-copy, zero-allocation parser functions per schema format
- [x] Generate typed Rust structs matching schema fields (with enum coercions)
- [x] Golden-file tests comparing generated code across schema changes
- [x] Error reporting for invalid schemas (compile-time diagnostics)

## Phase 4 — Core Parsing Runtime (`tpt-telemetry-core`)
- [x] Define public API: feed raw log lines/bytes → typed structs
- [x] Wire generated parsers (from Phase 3) into runtime dispatch
- [x] Streaming line/frame reader for multi-gigabyte inputs (chunked, zero-copy)
- [x] Verify zero heap allocations in steady-state parse loop (allocation-tracking test harness, e.g. custom `GlobalAlloc` counter)

## Phase 5 — Syslog Server (`tpt-syslog-server`)
- [x] UDP receiver (RFC3164 + RFC5424 framing)
- [x] TCP receiver (RFC5424 octet-counting / non-transparent framing)
- [x] Kernel-level `SO_RXQ_OVFL` drop-counter integration (Linux)
- [x] Ring-buffer backpressure mechanism to bound memory under log floods
- [x] Integration tests: high-throughput send/receive, overflow/backpressure behavior
- [x] Graceful shutdown / connection lifecycle handling

## Phase 6 — Security & Safety
- [x] Log injection prevention: sanitize extracted fields before downstream SIEM query rendering
- [x] PII redaction: schema-level annotations to hash/mask emails, IPs, credit card numbers
- [x] Security-focused test suite (injection payloads, redaction correctness)
- [x] Dependency audit (`cargo audit`) baseline — `cargo-audit-baseline.json` (0 vulnerabilities / 203 deps, 2026-08-13)

## Phase 7 — OTLP Export
- [x] Define internal typed-log → OTLP log record mapping
- [x] Implement OTLP/gRPC exporter (tonic-based)
- [x] Implement OTLP/HTTP+protobuf exporter
- [x] Config-driven transport selection (gRPC vs HTTP)
- [x] Batching/retry/backoff for exporters
- [ ] Integration tests against a local OTLP collector (e.g. otel-collector in CI)

## Phase 8 — Performance Validation
- [x] Criterion benchmark suite across grok engine, compiler-generated parsers, and end-to-end pipeline
- [x] Allocation-tracking CI gate asserting zero heap allocations in steady-state loop (opt-in `alloc-counter` feature + `zero_alloc_steady_state_match_loop` test)
- [x] Fuzz testing: `cargo-fuzz` targets (`schema_parser`, `grok_engine`, `syslog_framing`) in `fuzz/` + runnable fuzz-smoke tests in `cargo test`
- [x] Load test harness validating 1M lines/sec/core target — `throughput_bench` measures ~1.72M lines/sec/core (above target)
- [x] Document benchmark methodology and results (perf report) — `PERFORMANCE.md`

## Phase 9 — LLM-Assisted Inference (`tpt-inference`)
- [x] Define inference-provider trait/interface (sample logs in → suggested `.tpt-log` schema out)
- [x] Implement Claude API integration for schema suggestion from raw log samples
- [x] Prompt design + validation loop (suggested schema must compile via Phase 3 compiler)
- [x] CLI or library entry point for `tpt-inference`
- [x] Tests with representative log samples (Cisco ASA, generic CEF/LEEF, RFC5424)

## Phase 10 — Documentation & Examples
- [x] Top-level architecture doc (mirrors spec.txt's component diagram) — `ARCHITECTURE.md`
- [x] `.tpt-log` schema authoring guide + Grok pattern migration guide (from Logstash/ES) — `SCHEMA_GUIDE.md`
- [x] Example schemas (Cisco ASA, generic CEF, RFC5424) with sample logs — `examples/`
- [x] End-to-end example: syslog server → parser → OTLP export *(parser + example schemas done; syslog server & OTLP export landed in Phases 5/7; live-collector e2e deferred)*
- [x] API docs (`cargo doc`) polish pass per crate — 0 intra-doc warnings across workspace

## Phase 11 — CI/CD & Release
- [ ] CI pipeline: build, test, lint (`clippy`, `fmt`), audit, fuzz smoke tests
- [ ] Cross-platform build matrix (Linux primary for `SO_RXQ_OVFL`; document platform gaps)
- [ ] Versioning strategy across workspace crates (independent vs lockstep)
- [ ] crates.io metadata (description, keywords, categories, license fields) per crate
- [ ] Publish workflow (`cargo publish` order respecting inter-crate dependencies)
- [ ] Tag/release process + CHANGELOG conventions
