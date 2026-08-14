# tpt-telemetry-schema

The `.tpt-log` schema DSL for [`tpt-telemetry`](https://github.com/tpt-solutions/tpt-telemetry):
the grammar, AST, parser, and standard Grok pattern library used to describe log
formats, field extractions, and type coercions.

Security and network devices emit unstructured Syslog, CEF, or LEEF. A `.tpt-log`
schema turns those messy legacy lines into strongly-typed field captures. This
crate is the foundation every other crate builds on — it produces the `Schema` AST
that the compiler lowers into a zero-copy runtime parser.

## Crate position

```
.tpt-log file ──▶ tpt-telemetry-schema ──▶ Schema (AST)
                        │                          │
                        ├─ Grok pattern library    └─▶ tpt-telemetry-compiler
                        └─ parse() / load_file()
```

## Features

- **Expressive DSL** — `format` blocks with a `pattern:` template mixing literals,
  native typed captures (`%{field:type}`), and standard Grok captures
  (`%{IP:client}`).
- **Type system** — `int`, `uint`, `float`, `bool`, `string`, `ip`, `ipv4`,
  `ipv6`, `mac`, `timestamp`.
- **Coercions** — `coerce <field> to <type>` or to a named `enum { A, B, C }`
  (numeric severity `6` → `INFO`).
- **Derived fields** — `extract <field> from <source> using regex "..."` pulls
  sub-fields out of a captured value.
- **Redaction** — `redact <field> with mask|hash` for PII-safe export.
- **Grok compatibility** — ships the Logstash base + ECS subset pattern library, so
  `%{PATTERN:field}` references work unchanged when migrating from Logstash.
- **Robust parser** — never panics on adversarial/garbage input (fuzz-smoke
  tested); errors are surfaced as `SchemaError`.

## Installation

```toml
[dependencies]
tpt-telemetry-schema = "0.1.0"
```

## Usage

### Parsing a schema string

```rust
use tpt_telemetry_schema::parse;

let src = r#"
    format CiscoASA {
      pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
      coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
      redact message with mask;
    }
"#;

let schema = parse(src).expect("valid schema");
assert_eq!(schema.formats.len(), 1);
assert_eq!(schema.formats[0].name, "CiscoASA");
```

### Loading from a file

```rust
use tpt_telemetry_schema::load_file;

let schema = load_file("examples/schemas/cisco_asa.tpt-log")?;
println!("formats: {:?}", schema.formats.iter().map(|f| &f.name).collect::<Vec<_>>());
```

### Inspecting the AST

```rust
use tpt_telemetry_schema::{parse, ast::PatternPart};

let schema = parse(r#"format G { pattern: "%{IP:client} %{NUMBER:bytes:int}"; }"#).unwrap();
let parts = &schema.formats[0].pattern.parts;
let captures: usize = parts.iter().filter(|p| matches!(p, PatternPart::Capture(_))).count();
assert_eq!(captures, 2);
```

## The `.tpt-log` DSL

A file is a collection of `format` blocks. See [`SCHEMA_GUIDE.md`](https://github.com/tpt-solutions/tpt-telemetry/blob/main/SCHEMA_GUIDE.md)
and the `examples/schemas/` directory (Cisco ASA, RFC5424, CEF) for full details.

| Construct | Example | Meaning |
|-----------|---------|---------|
| Native capture | `%{severity:int}` | typed, zero-copy field |
| Grok capture | `%{IP:client}` | reference a library pattern |
| Grok + coercion | `%{NUMBER:bytes:int}` | grok match, then coerce |
| `coerce` | `coerce severity to enum { … }` | map value → variant index |
| `extract` | `extract ip from message using regex "\b\d{1,3}…\b"` | derived field |
| `redact` | `redact message with mask` | mask or hash at export |

Native captures backtrack to resolve delimiters correctly; the pattern must match
the entire line.

## API overview

- `parse(&str) -> Result<Schema, SchemaError>` — parse a schema from source.
- `load_file(path) -> Result<Schema, SchemaError>` — parse a schema from a file.
- `ast::Schema` / `ast::Format` / `ast::Pattern` / `ast::PatternCapture` / `ast::Extract`
  / `ast::Coercion` / `ast::Redaction` — the AST.
- `ast::TypeName`, `ast::CoercionTarget`, `ast::RedactMode` — type/redaction enums.
- `patterns` — the bundled Grok pattern library (Logstash base + ECS subset).

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Copyright TPT Solutions.
