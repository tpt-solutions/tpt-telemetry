# Changelog

All notable changes to `tpt-daemon` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- `--version` / `-V` and `--help` / `-h` flags.
- `--check` / `--validate-config` subcommand: loads and interpolates the config
  (no socket binds) and reports the resolved settings with header values
  redacted.
- `--healthcheck` subcommand: probes `GET /healthz` on the metrics port for use
  as a shell-free container `HEALTHCHECK` (the distroless runtime has no shell).
- `tpt-send-log` helper binary: fires sample syslog lines (UDP/TCP, from
  `--message`/`--file`/`--stdin`) at a running daemon.

## [0.1.0] - 2026-08-13

### Added
- Initial release: unified syslog ingest (UDP/TCP, RFC3164/RFC5424) -> schema
  parsing -> OTLP export daemon with a Prometheus `/metrics` and `/healthz`
  endpoint, TOML config with `${ENV_VAR}` secret interpolation, and graceful
  shutdown via `ctrlc`.
