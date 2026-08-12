# Contributing to tpt-telemetry

Thanks for your interest in improving `tpt-telemetry`! This document covers the
basics for building, testing, and submitting changes.

## Workspace layout

| Crate | Purpose |
|-------|---------|
| `tpt-telemetry-schema` | `.tpt-log` DSL: grammar, AST, parser, standard Grok pattern library. |
| `tpt-grok-engine` | SIMD-accelerated Grok pattern matcher (regex + `memchr` fast-scan). |
| `tpt-telemetry-compiler` | Schema AST → zero-copy runtime parser + Rust codegen. |
| `tpt-telemetry-core` | Runtime dispatch API, streaming reader, allocation harness. |
| `tpt-syslog-server` | High-throughput UDP/TCP syslog receiver (RFC3164 / RFC5424). |
| `tpt-inference` | LLM-assisted `.tpt-log` schema suggestion from raw log samples. |

## Building & testing

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets
cargo fmt   --all
```

### Allocation-tracking gate

The zero-allocation steady-state guarantee is verified by a gated test:

```bash
cargo test -p tpt-telemetry-core --features alloc-counter
```

This installs a thread-local counting global allocator and asserts that the
framing + matching hot loop performs **no** heap allocations after warmup.

### Codegen golden file

`cargo run -p tpt-telemetry-compiler --example gen_golden` regenerates
`tests/golden/cisco_asa.rs`. If you change the code generator, either update the
golden file or confirm the diff is intentional.

## Commit / PR guidelines

- Keep commits focused; explain *why*, not just *what*.
- Run `cargo clippy` and `cargo fmt` before opening a PR.
- New matcher features in `tpt-telemetry-compiler` should include a unit test
  and, where relevant, an example schema under `examples/schemas/`.
- Bug fixes should ship with a regression test.

## License

Contributions are accepted under the dual MIT OR Apache-2.0 license (see
`LICENSE-MIT` and `LICENSE-APACHE`). By contributing you agree to license your
work under these terms.
