# Authoring `.tpt-log` schemas

A `.tpt-log` file is a collection of `format` blocks. Each block declares how to
recognise and extract fields from one class of log line.

## Anatomy of a format

```tpt-log
format CiscoASA {
  pattern: "%ASA-%{severity:int}-%{msg_id:int}: %{message:string}";
  coerce severity to enum { EMERGENCY, ALERT, CRITICAL, ERROR, WARNING, NOTICE, INFO, DEBUG };
  redact message with mask;
}
```

- **`pattern:`** — The primary line template. It is a mix of:
  - **Literals** — verbatim text that must appear (`%ASA-`, `: `, `|`).
  - **Native captures** `%{field:type}` — a named field with a scalar type
    (`int`, `uint`, `float`, `bool`, `string`, `ip`, `ipv4`, `ipv6`, `mac`,
    `timestamp`). Values are matched zero-copy.
  - **Grok captures** `%{PATTERN:field}` or `%{PATTERN:field:type}` — reference a
    standard Grok pattern from the library (e.g. `%{IP:client}`,
    `%{NUMBER:bytes:int}`). Common names (`IP`, `IPV4`, `IPV6`, `MAC`, `NUMBER`,
    `INT`, `PORT`, `WORD`, `HOSTNAME`, `EMAILADDRESS`, timestamp patterns) map to
    native types so they stay allocation-free. Names without a native mapping are
    rejected by the zero-copy compiler (use `tpt-grok-engine` for those).
- **`coerce <field> to <type>`** — re-parse a field as a different scalar.
- **`coerce <field> to enum { A, B, C }`** — map a value to a variant index.
  Numeric values index directly into the variant list (e.g. severity `6` →
  `INFO`); string values match by name.
- **`redact <field> with mask|hash`** — mask (preserve last two characters) or
  deterministically hash a field before export.

## Matching semantics

A pattern must match the **entire** line. Captures are greedy: a `string` capture
consumes as much as possible while still allowing the rest of the pattern to
match, so pipe- and space-delimited fields resolve to the correct segments.

## Migrating from Logstash / Elasticsearch

Replace Logstash `match => { "message" => "%{IP:client} ..." }` with a `.tpt-log`
`pattern:`. The standard Grok library mirrors the Logstash base + ECS subset, so
most `%{PATTERN:field}` references work unchanged. See `examples/schemas/` for
ready-to-use Cisco ASA, RFC5424, and CEF schemas, and `examples/samples/` for
sample logs.

## Validating your schema

```bash
cargo test -p tpt-telemetry-core --test e2e
```

The end-to-end test loads the example schemas and parses the sample logs through
the streaming reader.
