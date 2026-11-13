# 0004 - Lint policy: deny everything, suppress one item with a reason

- **Status**: accepted
- **Date**: 2026-11-12

## Context

A warning an agent can ignore is not a rule. The habits agents bring to Rust are known: `unwrap` everywhere, `println!` debugging, `clone` to dodge the borrow checker, a lock held across an `.await`, a wildcard `match` arm that swallows the next variant. Clippy can name each of them.

## Decision

1. **`[workspace.lints]`** sets clippy `all` and `pedantic` to deny, plus restriction and nursery lints picked one by one (the list and the reason for each exception are in `Cargo.toml`), and rustc's `unsafe_code` to forbid. Every member opts in with `[lints] workspace = true`.
2. **Tests may panic**: `clippy.toml` exempts tests from `unwrap_used`, `expect_used`, `panic`, and `indexing_slicing`.
3. **Size limits** live in `clippy.toml`: 80 lines per function, cognitive complexity 15, nesting 4, six arguments, one bool parameter.
4. **A suppression is `#[expect(lint, reason = "...")]` on the one item that needs it.** `#[allow]` is refused (`allow_attributes`), a suppression without a reason is refused (`allow_attributes_without_reason`), and a crate-wide `#![allow]` is refused by a source rule. `#[expect]` fails once the lint no longer fires, so a stale suppression cannot linger.
5. **Banned calls with a replacement** are `disallowed-methods` in `clippy.toml`, each with the reason that names what to use instead (ADR-0009).

## Consequences

- Positive: an agent meets every finding at once, with the fix in the message, before a reviewer does.
- Negative: pedantic lints occasionally ask for churn; the four that fight these conventions are allowed in `Cargo.toml` with a reason.
- Neutral: a new clippy release can add findings, so moving the toolchain pin is a deliberate change.

## Alternatives considered

- **Warn instead of deny**: warnings are ignored.
- **Per-crate lint tables**: Cargo refuses to mix `workspace = true` with overrides, by design; one table is easier to review.
