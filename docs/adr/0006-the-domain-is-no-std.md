# 0006 - The domain is no_std

- **Status**: accepted
- **Date**: 2026-11-12

## Context

The domain must hold rules and values only: no I/O, no threads, no environment, no clock. In go-base this needed a list of allowed standard-library packages and an analyzer for clock reads. Rust can make the compiler say it.

## Decision

1. **Every domain crate starts with `#![no_std]`** and uses `alloc` for `String`, `Vec`, and `BTreeMap`. Files, sockets, threads, environment variables, and `SystemTime::now` do not exist there.
2. **`extern crate std` is allowed only under `#[cfg(test)]`**, checked by a source rule.
3. **External crates are allowlisted**: `thiserror` (without its std feature) and `jiff` (without default features), in `architecture.toml`.
4. **Time arrives as a `jiff::Timestamp` value**, read by a use case from the clock port.

## Consequences

- Positive: purity is a compile error, not a review comment.
- Negative: `HashMap` needs std; the domain uses `BTreeMap`. A crate the domain wants must support `no_std`, or the allowlist question becomes an ADR.
- Neutral: test code runs with std as usual.

## Alternatives considered

- **clippy `disallowed-methods` per domain crate**: a per-crate `clippy.toml` replaces the root one instead of merging with it, so every domain would carry a copy of every ban.
- **A source rule listing forbidden std paths**: incomplete by construction; `no_std` is complete.
