# tpt-telemetry-core

Core parsing runtime and streaming line/frame reader for
[`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Wires the compiled schema (`tpt-telemetry-compiler`) into a single `Parser`
dispatch API and provides a chunked, allocation-reusing `StreamReader` for
multi-gigabyte inputs. The match hot path is zero-copy (field values are borrowed
from the input line) and the steady-state loop can run with no heap allocation when
reusing a `MatchCtx`.

## Features

- **`Parser` dispatch** — compile a `Schema` once, then dispatch each line to the
  first format that matches.
- **Fully typed records** — `parse_line` yields coerced + redacted `Record`s.
- **Zero-allocation hot path** — `match_line` / `matches` reuse a `MatchCtx` so the
  steady-state loop performs no heap allocation once warmed up.
- **`StreamReader`** — newline-delimited (with optional trailing `\r`) framing over
  any `Read` source; grows its internal buffer only when a line exceeds capacity.
- **Allocation tracking** — the `alloc-counter` feature installs a counting global
  allocator to assert the framing + matching loop is allocation-free.
- **Robust I/O** — clean EOF vs. genuine transport error distinguished via
  `last_error()`.

## Installation

```toml
[dependencies]
tpt-telemetry-core = "0.1.0"
```

## Usage

### Parsing single lines

```rust
use tpt_telemetry_core::{parse, Parser, Value};

let schema = parse(r#"
    format CiscoASA {
      pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
      coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
    }
"#).unwrap();

let parser = Parser::new(schema).unwrap();
let rec = parser.parse_line("%ASA-6-302013: Built inbound TCP connection").unwrap();
assert_eq!(rec.format, "CiscoASA");
let sev = rec.fields.iter().find(|f| f.name == "severity").unwrap();
assert_eq!(sev.value, Value::Enum(6));
```

### Zero-allocation matching

```rust
use tpt_telemetry_core::{parse, Parser, MatchCtx};

let parser = Parser::new(parse("%ASA-%{severity:int}-%{msg_id:int}: %{message:string}").unwrap()).unwrap();
let mut ctx = MatchCtx::new(8);

assert!(parser.matches("%ASA-3-106001: connection denied", &mut ctx));
let raw = parser.match_line("%ASA-3-106001: connection denied", &mut ctx).unwrap();
println!("matched format: {}", raw.format);
```

### Streaming a large file

```rust
use std::io::Cursor;
use tpt_telemetry_core::StreamReader;

let data = b"line one\nline two\r\nline three\n";
let mut r = StreamReader::new(Cursor::new(&data[..]));
while let Some(line) = r.next_line() {
    println!("{}", String::from_utf8_lossy(line));
}
assert!(r.last_error().is_none()); // clean EOF
```

## The `alloc-counter` feature

```bash
cargo test -p tpt-telemetry-core --features alloc-counter
```

Enables a counting global allocator. The `zero_alloc_steady_state_match_loop`
test verifies the framing + matching hot path performs **zero** new allocations
once warmed up.

## API overview

- `Parser::new(Schema)`, `Parser::from_compiled(CompiledSchema)` — construct.
- `Parser::parse_line(&str) -> Option<Record>` — coerced + redacted record.
- `Parser::match_line(&str, &mut MatchCtx) -> Option<RawMatch>` — borrowed match.
- `Parser::matches(&str, &mut MatchCtx) -> bool` — match test.
- `Parser::schema() -> &CompiledSchema` — access the backing compiled schema.
- `StreamReader::new(reader)` / `with_capacity(reader, cap)` — streaming framing.
- `StreamReader::next_line() -> Option<&[u8]>`, `last_error() -> Option<&io::Error>`.
- Re-exports: `CompiledSchema`, `Record`, `Field`, `Value`, `MatchCtx`, `load_file`,
  `parse`, `Schema`.
- `alloc_count()` / `reset_alloc()` — available under the `alloc-counter` feature.

## Benchmarks

```bash
cargo bench -p tpt-telemetry-core   # e2e, throughput
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
