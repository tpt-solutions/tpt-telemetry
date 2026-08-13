# Versioning Strategy

This document defines how the `tpt-telemetry` workspace is versioned and
released. It is part of Phase 11 (CI/CD & Release) of `todo.md`.

## Decision: locked-step workspace versioning

All publishable crates share a **single version number**, sourced from the
`[workspace.package]` table:

```toml
# root Cargo.toml
[workspace.package]
version = "0.1.0"
```

Each crate inherits it:

```toml
[package]
name = "tpt-grok-engine"
version.workspace = true
# ...
```

### Why lockstep (not independent versions)

The crates form a tightly coupled, single-product dependency graph:

```
tpt-telemetry-schema            (leaf)
tpt-grok-engine        -> schema
tpt-telemetry-compiler -> schema
tpt-telemetry-core     -> schema, grok-engine, compiler
tpt-syslog-server      -> core
tpt-inference          -> schema, compiler
tpt-otlp               -> compiler
```

- Core consumes `schema`, `grok-engine`, and `compiler` directly.
- Every feature release of the parser is effectively a release of the whole
  stack, because the generated-code contract (`CompiledSchema`, zero-copy
  `Seg`ments) is shared across `compiler`/`core`/`otlp`.
- Independent (SemVer-per-crate) versioning would create a permutation of
  compatible version ranges for consumers to reason about for little benefit,
  since the crates are not independently useful outside the workspace yet.

Lockstep keeps the public contract simple: "pin `tpt-telemetry-*` crates to the
same version" is the only rule consumers need.

### When this could change

If a crate graduates into a broadly reusable, independently consumed library
(e.g. `tpt-grok-engine` used standalone by another project), it can be moved to
an independent `version = "x.y.z"` in its own `Cargo.toml`. Until then, keep the
workspace-version inheritance.

## Release flow (lockstep)

1. Land all changes on `main`.
2. Bump `version` once, in the root `[workspace.package]` only. This propagates
   to every crate.
3. Update per-crate `CHANGELOG` entries (see *Changelogs* below).
4. Tag the release: `git tag -s v<version> -m "tpt-telemetry <version>"`.
5. Publish dependencies before dependents (cargo enforces this, but the order
   is): `schema` → `grok-engine`, `compiler` → `core` → `syslog-server`,
   `inference`, `otlp` → `daemon`.
   `.github/workflows/publish.yml` publishes the crates in this exact order on a
   `v*` tag push (with retries to absorb crates.io index propagation lag).
6. Push the tag and the `v<version>` GitHub Release (auto-generated notes from
   the changelogs).

## Changelogs

Each publishable crate carries a `CHANGELOG.md` at its crate root, following
[Keep a Changelog](https://keepachangelog.com/) conventions. The `## [Unreleased]`
section is moved under the new version heading at release time. `cargo-deny` /
the publish workflow can be configured to fail if a versioned release has an
empty changelog entry.

## Pre-1.0 note

While the version is `0.y.z`, SemVer treats any `y` bump as breaking. This gives
us latitude to make incompatible API changes within the `0.x` line. The lockstep
rule still applies: every crate moves together.
