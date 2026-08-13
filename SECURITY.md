# Security Policy

## Supported Versions

`tpt-telemetry` is pre-1.0 (`0.1.x`). Only the latest `0.1.x` release line
receives security fixes. Breaking security-relevant changes may occur between
`0.1.x` releases and will be noted in the changelog.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | ✅        |
| < 0.1   | ❌        |

## Reporting a Vulnerability

Please report security vulnerabilities **privately** rather than opening a
public issue.

- Use **GitHub Security Advisories** for this repository: open a draft advisory
  at `https://github.com/tpt-solutions/tpt-telemetry/security/advisories/new`.
- If you cannot use GitHub Security Advisories, email the maintainers (see
  `Repository` / `Homepage` in `Cargo.toml`) with `[SECURITY]` in the subject.

We aim to acknowledge reports within **5 business days** and to provide a fix or
mitigation plan within **30 days** of a confirmed, reproducible issue.

Please include:

- Affected crate(s) and version(s).
- A description and, if possible, a minimal reproduction.
- The potential impact (e.g. memory exhaustion, credential leakage, log
  injection into a downstream SIEM).

## Security Hardening Already in Place

- **Syslog framing caps** (`tpt-syslog-server`): octet-counting lengths and
  LF-delimited frames are bounded by `max_frame_len`; oversized frames are
  rejected rather than buffered without limit.
- **Connection caps** (`tpt-syslog-server`): `max_connections` rejects surplus
  TCP connections (counted in `rejected_connections`).
- **PII redaction / log-injection sanitization** (`tpt-telemetry-compiler`):
  schema-level `mask`/`hash` redactions and control-character stripping before
  downstream rendering.
- **TLS for OTLP** (`tpt-otlp`): compile with the `tls` feature and use an
  `https://` endpoint. The `require_tls` exporter flag hard-errors when auth
  headers are configured over a plaintext `http://` transport. Header values are
  redacted from `Debug` output.
- **Dependency auditing**: `cargo audit` baseline is tracked in
  `cargo-audit-baseline.json` (0 vulnerabilities / 203 deps). Run
  `cargo audit` in CI.

## Supply Chain

- All crates are released under `MIT OR Apache-2.0`.
- CI runs `cargo audit`, `clippy -D warnings`, `rustfmt --check`, and the test
  suite on every push/PR (see `.github/workflows/ci.yml`).
