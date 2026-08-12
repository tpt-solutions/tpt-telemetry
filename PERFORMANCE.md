# Performance Validation — tpt-telemetry

Benchmark methodology, harness layout, and observed results for the
`From raw bytes → typed `Record`` pipeline. This covers Phase 8 of `todo.md`.

## 1. Harness layout

All benchmarks use [Criterion](https://github.com/japaric/criterion.rs) `0.5`
and live under each crate's `benches/` directory (built with
`harness = false` so Criterion owns argument parsing):

| Benchmark crate / file | Measures |
| --- | --- |
| `tpt-grok-engine/benches/grok_bench.rs` | Grok baseline vs SIMD `memchr` pre-scan hot paths; compound IP/WORD/NUMBER patterns. |
| `tpt-telemetry-compiler/benches/compile_bench.rs` | Schema compile (`CompiledSchema::compile`) + single-line parse of the zero-copy matcher. |
| `tpt-telemetry-core/benches/e2e_bench.rs` | Full runtime path: `Parser::new` once, then `Parser::parse_line` dispatch. |
| `tpt-telemetry-core/benches/throughput_bench.rs` | Sustained load: parse a 4 096-line corpus + `StreamReader` framing throughput (lines/sec). |

### Allocation-tracking gate

A counting `GlobalAlloc` (opt-in via the `alloc-counter` feature in
`tpt-telemetry-core`) asserts **zero heap allocations** in the steady-state
match loop. The gate is the test
`zero_alloc_steady_state_match_loop`, run in CI with:

```bash
cargo test -p tpt-telemetry-core --features alloc-counter zero_alloc
```

### Fuzz smoke tests

`cargo-fuzz` targets live in the standalone `fuzz/` workspace
(`schema_parser`, `grok_engine`, `syslog_framing`); they require the nightly
toolchain and `cargo fuzz`:

```bash
cd fuzz && cargo +nightly fuzz run schema_parser
```

Runnable (stable-toolchain) fuzz-smoke tests are also wired into the normal
`cargo test` suite so regressions are caught without nightly:

- `tpt-telemetry-schema` → `parser_never_panics_on_garbage`
- `tpt-syslog-server` → `framing_never_panics_on_random_streams`

## 2. Running

```bash
# All benchmarks, default measurement settings:
cargo bench

# Specific suite, faster turnaround:
cargo bench -p tpt-telemetry-core --bench throughput_bench
```

HTML reports land in `target/criterion/`.

## 3. Observed results

Environment: Windows 11 (win32), Rust 1.97.1, release profile, Criterion
`--sample-size 10 --measurement-time 3` (short measurement; treat as
indicative, not a formal reference run). Numbers are median wall-clock.

### Grok engine (`tpt-grok-engine`)

| Benchmark | Median | Note |
| --- | --- | --- |
| `grok_baseline_match` | ~669 ns | full `regex::Regex` match |
| `grok_simd_scan_match` | ~660 ns | `memchr` pre-scan + regex on a hit |
| `grok_simd_scan_miss` | ~31 ns | `memchr` rejects non-matching line before regex |
| `grok_ip_compound` | ~712 ns | `%{IP} %{WORD} %{NUMBER:int}` compound |

The SIMD pre-scan path is ~20× faster on misses (it avoids the regex engine
entirely), confirming the two-stage hot-path design.

### Compiler (`tpt-telemetry-compiler`)

| Benchmark | Median | Note |
| --- | --- | --- |
| `compiler_compile_schema` | ~6.1 µs | AST → `CompiledSchema` (one-time per schema) |
| `compiler_parse_line` | ~532 ns | zero-copy typed parse of one line |

### Core end-to-end (`tpt-telemetry-core`)

| Benchmark | Median | Note |
| --- | --- | --- |
| `core_parse_line_e2e` | ~523 ns | `Parser::parse_line` over a Cisco ASA line |

### Throughput / load (`tpt-telemetry-core`)

| Benchmark | Median | Lines/sec (elem throughput) |
| --- | --- | --- |
| `core_load_parse_corpus` | 2.38 ms / 4 096 lines | **~1.72 M lines/sec** |
| `core_streamreader_framing` | 153 µs / 4 096 lines | ~26.8 M lines/sec (framing only) |

## 4. Assessment vs target

The Phase 8 reference target is **1M lines/sec/core** for the parse pipeline.
The measured `core_load_parse_corpus` throughput is **~1.72 M lines/sec** on a
single core — comfortably above target. The `StreamReader` framing stage is
~15× faster than full parse, confirming it is not the bottleneck; parse + typed
coercion dominates steady-state cost, as expected for a regex/segment matcher.

> Note: these are short, non-formal measurements on one machine. A formal
> reference run (longer `--measurement-time`, pinned reference hardware,
> multiple schema shapes) should be captured in CI and the table refreshed.

## 5. Status

- [x] Criterion benchmark suite: grok engine, compiler-generated parsers, e2e + throughput.
- [x] Allocation-tracking gate asserting zero heap allocations in steady-state loop.
- [x] Fuzz targets (`schema_parser`, `grok_engine`, `syslog_framing`) + runnable fuzz-smoke tests.
- [x] Load harness reporting lines/sec (exceeds 1M lines/sec/core reference target).
- [x] This methodology + results report.
