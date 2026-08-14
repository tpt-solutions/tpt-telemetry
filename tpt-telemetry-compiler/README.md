# tpt-telemetry-compiler

`build.rs` codegen: `.tpt-log` schema AST → zero-copy Rust parsers, for
[`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry).

Lowers a parsed [`Schema`] into a `CompiledSchema`: a flat list of `Seg`ments
(literals + typed captures). At run time the matcher borrows field values
directly from the input line. Also emits Rust source via [`codegen`] for
golden-file testing and `build.rs` integration.

## Crate position

```
Schema (AST, from tpt-telemetry-schema)
        │  CompiledSchema::compile
        ▼
CompiledSchema ──▶ Parser (tpt-telemetry-core) ──▶ StreamReader
        │
        └─▶ codegen::generate_rust ──▶ build.rs include!
```

## Features

- **Zero-copy matcher** — captured field values are `&str` slices into the input
  line; numeric / timestamp / enum coercions are parsed in place. No per-line heap
  allocation in the steady-state loop (see `tpt-telemetry-core`'s `alloc-counter`).
- **Typed `Record`s** — each match yields coerced, redacted fields carrying a
  `Value` (enum index, string, etc.).
- **Rust codegen** — emit a self-contained parser module from a schema, for
  golden-file diffing and `build.rs` embedding.
- **Security gate** — `security` module validates patterns before lowering them.
- **Re-exports** — exposes the `tpt-telemetry-schema` AST types the generated
  code references directly (`CoercionTarget`, `RedactMode`, `TypeName`, `Schema`).

## Installation

```toml
[dependencies]
tpt-telemetry-compiler = "0.1.0"
```

## Usage

### Compile and parse in-process

```rust
use tpt_telemetry_compiler::{parse, CompiledSchema, Value};

let src = r#"
    format CiscoASA {
      pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
      coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
    }
"#;

let schema = parse(src).unwrap();
let cs = CompiledSchema::compile(&schema).unwrap();
let rec = cs.parse_line("%ASA-6-302013: Built inbound TCP connection").unwrap();

assert_eq!(rec.format, "CiscoASA");
let sev = rec.fields.iter().find(|f| f.name == "severity").unwrap();
assert_eq!(sev.value, Value::Enum(6)); // INFO
```

### `build.rs` integration

Generate a parser module at build time and `include!` it into your crate:

```rust,no_run
// build.rs
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("tpt_schema.rs");
    let src = tpt_telemetry_compiler::build::codegen_schema_file("schemas/asa.tpt-log")
        .expect("codegen failed");
    fs::write(&out, src).unwrap();
    println!("cargo:rerun-if-changed=schemas/asa.tpt-log");
}
```

```rust,no_run
// lib.rs
// include!(concat!(env!("OUT_DIR"), "/tpt_schema.rs"));
```

```toml
# Cargo.toml
[build-dependencies]
tpt-telemetry-compiler = "0.1.0"
```

You can also codegen from an in-memory string via `build::codegen_schema_str`.

## API overview

- `build::codegen_schema_file(path) -> Result<String>` / `codegen_schema_str(&str)`
  — generate Rust source for a schema.
- `parse(&str)`, `Schema` — re-exported from `tpt-telemetry-schema`.
- `CompiledSchema`, `CompiledFormat`, `Seg`, `Field`, `TypedField`, `Record`,
  `RawMatch`, `MatchCtx`, `Value` — the compiled runtime model.
- `error::CompileError` / `error::Result` — compile/codegen errors.
- `codegen`, `security` — Rust code generation and pre-compile validation.

## Benchmarks

```bash
cargo bench -p tpt-telemetry-compiler   # compile_schema, parse_line
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
