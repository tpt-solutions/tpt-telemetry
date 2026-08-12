# Platform Support & CI Build Matrix

`tpt-telemetry` is a Cargo workspace. This document records which platforms are
CI-validated, where the platform-specific code lives, and the known gaps.

## Build matrix

The CI pipeline (`.github/workflows/ci.yml`) runs the following jobs:

| Job            | OS matrix                                  | Toolchain    | Purpose                                  |
|----------------|--------------------------------------------|--------------|------------------------------------------|
| `fmt`          | `ubuntu-latest`                            | stable       | `cargo fmt --all -- --check`             |
| `clippy`       | `ubuntu-latest`                            | stable       | `cargo clippy -D warnings` (all feats)  |
| `build-test`   | `ubuntu-latest`, `macos-latest`, `windows-latest` | stable | build + `cargo test` on each OS     |
| `audit`        | `ubuntu-latest`                            | stable       | `cargo audit` (RustSec)                  |
| `fuzz`         | `ubuntu-latest`                            | nightly      | `cargo fuzz run` smoke (50k runs each)   |

**Linux is the primary supported platform.** All platform-specific behaviour is
exercised there, and the full feature set (including the syslog overflow
counter) is validated. macOS and Windows are part of the matrix to catch
OS-portability regressions in the shared code, but are treated as secondary.

## Platform gaps

### `SO_RXQ_OVFL` — Linux only

`tpt-syslog-server` surfaces a kernel-reported socket receive-queue overflow
count via the Linux-only `SO_RXQ_OVFL` socket option. This integration is
guarded by `#[cfg(target_os = "linux")]` in `tpt-syslog-server/src/server.rs`:

- On **Linux**, the overflow count is enabled and surfaced via the stats API
  (`RecvStats.rxq_overflow`, see `stats.rs`).
- On **macOS / Windows**, the `SO_RXQ_OVFL` code is compiled out. The server
  still builds and runs; the overflow counter is reported as `None`/unavailable.
  No other functionality is affected.

There is no equivalent kernel overflow counter on macOS or Windows, so this gap
is by design rather than a missing implementation. If a platform-agnostic
overflow signal is needed later, it would have to be approximated in user space
(e.g. a recv-loop missed-packet heuristic).

### Other considerations

- The `grpc` feature of `tpt-otlp` (tonic + tokio) is feature-gated and is
  compiled on all three CI platforms via `cargo build --workspace
  --all-features`. It is not enabled by default.
- The fuzz targets require `cargo-fuzz` and a nightly toolchain, so the `fuzz`
  job runs only on Linux/nightly. The same code paths are covered on all
  platforms by the regular `#[test]` suites in `cargo test`.
- Memory/allocation guarantees (`alloc-counter` zero-alloc gate) are validated
  only on the Linux CI runner.

## Local development

```bash
# Every platform
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Linux-only feature surface (no-op build on other OSes)
cargo build -p tpt-syslog-server

# Fuzzing (requires nightly + cargo-fuzz)
cargo +nightly fuzz run schema_parser  -- -runs=50000
cargo +nightly fuzz run grok_engine    -- -runs=50000
cargo +nightly fuzz run syslog_framing -- -runs=50000
```
