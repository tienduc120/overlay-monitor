# 0011 - Errors are values owned by the layer that raises them

- **Status**: accepted
- **Date**: 2026-11-12

## Context

A layered service fails in three ways: a business rule is broken, a use case cannot do what it was asked, and a dependency misbehaves. If they share one error type, delivery decides what a storage failure means or the domain learns about HTTP.

## Decision

1. **Failure is a returned `Result`.** An expected failure is never a panic; clippy refuses `unwrap`, `expect`, and `panic` outside tests.
2. **Each layer owns the errors it raises**, as `thiserror` enums: the domain for broken invariants (`NoteError::EmptyName`), the application for each use case's outcomes (`CreateNoteError::{Invalid, AlreadyExists, Storage}`), a port for conditions its callers branch on (`SaveError::AlreadyExists`).
3. **A layer translates at its boundary and keeps the cause** as `#[source]` or `#[from]`; `map_err` that drops the error is refused by clippy.
4. **Only `interfaces` maps an outcome to a status**, with an exhaustive `match`: a new variant does not compile until someone decides its status (the wildcard arm is refused). An internal error is logged with its cause and answered with a generic body.
5. **`anyhow` is for the tool only**, enforced by `deny.toml` and `architecture.toml`.

## Consequences

- Positive: a new transport maps the same outcomes without touching a use case; a status is decided in one place.
- Negative: every boundary writes a small translation.

## Alternatives considered

- **`anyhow` everywhere**: callers cannot branch on an outcome without downcasting.
- **One shared error enum**: the domain would carry transport kinds it must not know about.
- **snafu or error-stack**: a second vocabulary for what `thiserror` says.
