# AGENTS.md

Guidance for AI agents (and humans) working in the `tpt-telemetry` repository.

## What this project is

A Rust Cargo workspace that turns unstructured security/network telemetry
(Syslog, CEF, LEEF) into strongly-typed records via a `.tpt-log` schema DSL,
then exports them as OpenTelemetry (OTLP). See `README.md`, `ARCHITECTURE.md`,
and `SCHEMA_GUIDE.md` for full context.

## Workspace layout

Declared in `Cargo.toml` (`[workspace] members`). Crates, in pipeline order:

- `tpt-telemetry-schema` — `.tpt-log` DSL grammar/AST/parser + standard Grok library.
- `tpt-grok-engine` — SIMD-accelerated Grok matcher (`memchr` fast-scan + `regex`).
- `tpt-telemetry-compiler` — AST → `CompiledSchema` (flat `Seg`s) + Rust codegen.
- `tpt-telemetry-core` — Public `Parser` API, `StreamReader`, allocation harness.
- `tpt-syslog-server` — UDP/TCP syslog receiver (RFC3164 / RFC5424) + backpressure.
- `tpt-inference` — LLM-assisted schema suggestion behind a provider trait.
- `tpt-otlp` — Typed record → OTLP mapping + HTTP/JSON and gRPC exporters.
- `tpt-daemon` — Unified binary wiring ingest → parse → OTLP (+ Prometheus metrics).

Non-crate dirs: `examples/` (sample schemas/runs), `fuzz/`, `packaging/`, `target/`.

## Build, test, lint (run from repo root)

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets
cargo fmt   --all
```

- Allocation-free guarantee is gated: `cargo test -p tpt-telemetry-core --features alloc-counter`.
- Codegen has a golden file: `cargo run -p tpt-telemetry-compiler --example gen_golden`
  regenerates `tests/golden/cisco_asa.rs`. Review/confirm any diff it produces.
- Toolchain is pinned in `rust-toolchain.toml` (Rust 1.85 / edition 2021). Use it; do not bump casually.

## Conventions to follow

- **Zero/low-allocation hot path is a hard constraint.** The matcher borrows `&str`
  slices from the input; `MatchCtx` is reuse-cleared, not reallocated. Avoid heap
  allocations in the framing/matching loop (`tpt-telemetry-core`, `tpt-telemetry-compiler`).
  Redaction (`mask`/`hash`) is the one permitted place that produces `OwnedString`.
- **Use workspace dependencies.** Crate interdeps and shared libs are declared under
  `[workspace.dependencies]` in `Cargo.toml`. Reference them as
  `dep = { workspace = true }`; do not add arbitrary version pins per-crate.
- **`.tpt-log` schemas** reference Grok patterns (Logstash base + ECS subset) like
  `%{IP:client}`. Schema grammar lives in `tpt-telemetry-schema`.
- **Codegen changes require golden-file review.** If you touch the compiler generator,
  regenerate and inspect `tests/golden/*`.
- **Errors** use `thiserror` (already a workspace dep). Prefer typed error enums.
- **Serde** (`serde = { workspace = true, features = ["derive"] }`) for serialization.
- Format with `cargo fmt` and pass `cargo clippy` before considering work done.

## PR / commit guidance (from CONTRIBUTING.md)

- Keep commits focused; explain *why*.
- New matcher features → unit test + example schema under `examples/schemas/`.
- Bug fixes → regression test.
- Dual-licensed MIT OR Apache-2.0; contributions are accepted under those terms.

## Secrets / config

`tpt-daemon` config is TOML with `${ENV_VAR}` secret interpolation. Never hardcode
secrets; never commit keys. AI provider keys for `tpt-inference` come from env.

## Before answering questions or coding

Read the relevant crate's `src/` and its `Cargo.toml` first. The architecture is
tightly coupled (schema → compiler → core → otlp); changes usually ripple across
the pipeline, so check all affected crates and run the full workspace test/clippy.
