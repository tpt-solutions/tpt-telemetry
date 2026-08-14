# tpt-grok-engine

A SIMD-accelerated Grok pattern matcher for [`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Compiles Grok pattern strings (with `%{NAME:field}` references from the
standard/ECS library) into `regex::Regex` source and matches them against log
lines. A two-stage hot path uses `memchr`'s vectorized substring search to reject
non-matching lines before running the full regex.

This is the engine behind arbitrary / ECS Grok compatibility. The
`tpt-telemetry-compiler` zero-copy matcher handles the common native-typed subset
without `regex` allocation; `tpt-grok-engine` is used for the rest.

## Features

- **Grok compatibility** — `%{IP:client}`, `%{NUMBER:bytes:int}`,
  `%{GREEDYDATA:message}`, etc., with the bundled Logstash base + ECS pattern set.
- **Nested expansion** — patterns expand recursively (`NUMBER` → `BASE10NUM`,
  `IP` → `IPV4|IPV6`), so the generated regex matches real-world input.
- **SIMD fast pre-scan** — `Grok::scan` uses `memchr`'s vectorized `memmem` to
  reject lines missing the longest mandatory literal run before regex execution.
- **Named captures** — access matched groups by name, value, range, or iterate all.
- **Debuggable** — `regex_source()` returns the compiled regex for inspection and
  golden-file testing.

## Installation

```toml
[dependencies]
tpt-grok-engine = "0.1.0"
```

## Usage

### Basic match

```rust
use tpt_grok_engine::Grok;

let g = Grok::new("%{IP:client} %{WORD:action}").unwrap();
let m = g.find("192.168.1.1 accepted").unwrap();
assert_eq!(m.get("client"), Some("192.168.1.1"));
assert_eq!(m.get("action"), Some("accepted"));
```

### SIMD-accelerated scan (hot path)

```rust
use tpt_grok_engine::Grok;

let g = Grok::new("%ASA-%{INT:severity}-%{NUMBER:msg_id}: %{GREEDYDATA:message}").unwrap();

// The fast literal "%ASA-" is absent → scan returns None immediately.
assert!(g.scan("random line here").is_none());

// Present → falls back to the full regex.
let m = g.scan("%ASA-3-106001: connection denied").unwrap();
assert_eq!(m.get("severity"), Some("3"));
assert_eq!(m.get("msg_id"), Some("106001"));
```

### Working with matches

```rust
use tpt_grok_engine::Grok;

let g = Grok::new("bytes=%{NUMBER:bytes:int}").unwrap();
let m = g.find("bytes=10423").unwrap();

assert_eq!(m.as_str(), "bytes=10423");
assert_eq!(m.get("bytes"), Some("10423"));
assert!(m.range("bytes").is_some());
for (name, value) in m.named() {
    println!("{name} = {value}");
}
```

### Introspection

```rust
use tpt_grok_engine::Grok;

let g = Grok::new("%{IP:src} %{NUMBER:port}").unwrap();
assert!(g.regex_source().contains("25[0-5]")); // IPV4 expansion present
let names: Vec<_> = g.capture_names().flatten().collect();
```

## API overview

- `Grok::new(pattern) -> Result<Grok, GrokError>` — compile a Grok pattern.
- `Grok::find(input) -> Option<Match>` — full regex match.
- `Grok::scan(input) -> Option<Match>` — SIMD pre-scan then `find`.
- `Grok::regex_source()`, `Grok::capture_names()` — introspection.
- `Match::as_str()`, `Match::get(name)`, `Match::range(name)`, `Match::named()` —
  access captured groups.
- `tokenize_pattern(pattern)` — re-exported tokenizer for tooling/tests.
- `error::GrokError` — compile/validation errors (e.g. unknown pattern names).

## Benchmarks

Run the Criterion suite (Grok baseline match, IP compound, SIMD scan hit/miss):

```bash
cargo bench -p tpt-grok-engine
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
