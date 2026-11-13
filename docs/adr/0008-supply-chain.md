# 0008 - Supply chain

- **Status**: accepted
- **Date**: 2026-11-12

## Context

Every dependency is code that runs with ours, a license we accept, and a version an agent can bump without understanding it.

## Decision

1. **`cargo deny check`** runs in `verify --full`: RustSec advisories and yanked versions, a license allowlist (MIT, Apache-2.0, BSD, ISC, Unicode-3.0, Zlib), crates.io as the only registry, no git sources, no wildcard versions.
2. **Banned crates** with the reason in `deny.toml`: `anyhow` outside `xtask`, `async-trait`, `lazy_static`, `openssl`, and the deprecated YAML crates.
3. **Unused dependencies** fail `verify --full` through cargo-shear.
4. **A new dependency is an ADR**: versions live once in the protected root `Cargo.toml`, so adding one needs the user's approval, and it is credited in the README.
5. **A bare `cargo update` is refused** by the agent guard; update one crate with `cargo update -p <crate> --precise <version>` and say why.

## Consequences

- Positive: a vulnerable, unlicensed, or unexpected crate fails before a push.
- Negative: an advisory against a transitive dependency can block unrelated work until it is updated or ignored with a reason.

## Alternatives considered

- **cargo-audit alone**: advisories only, no licenses or bans.
- **cargo-vet**: stronger, and a process to run; worth it when a project ships to customers.
