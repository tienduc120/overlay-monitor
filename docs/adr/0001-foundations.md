# 0001 - Foundations

- **Status**: accepted
- **Date**: 2026-11-12

## Context

AI agents extend this base, so correctness must come from tools, not from prose an agent may skip. The target is strict DDD and clean-architecture backend services that stay maintainable as services multiply. The shape follows go-base, the same author's Go base, rebuilt in Rust rather than translated.

## Decision

1. **Enforcement first.** The base ships almost no features. Every rule has an enforcer and a fixture that proves it fires; a rule without both is advice.
2. **Rust, latest stable, pinned.** `rust-toolchain.toml` pins 1.99.1 with rustfmt and clippy; rustup installs it on every machine and in CI. Edition 2024, resolver 3. Moving the pin is a new ADR.
3. **No Make.** `cargo xtask` (a crate in `xtask/`, run through a cargo alias) is the single command surface: it behaves the same on Windows, Linux, and CI, is unit tested, and gives agents one place to look.
4. **One workspace, one crate per layer per service.** `services/<svc>/{domain,application,infrastructure,interfaces,api}`, `platform/<name>`, and `xtask`. A layer violation is then a missing Cargo edge, which does not compile.
5. **Versions and lints are decided once**, in the root `Cargo.toml`; members only opt in with `workspace = true`.
6. **Pinned developer tools** (cargo-nextest, cargo-deny, cargo-shear, typos, lefthook) are checked by `cargo xtask doctor`, which reports and never installs. CI installs the same versions.
7. **AGENTS.md is the shared agent source**; `CLAUDE.md` imports it.

## Consequences

- Positive: one toolchain everywhere; rules that cannot erode silently; the compiler enforces most layering.
- Negative: more manifests than a single crate; Rust compiles slower than Go, which lengthens an agent's loop.
- Neutral: Windows needs the Visual Studio Build Tools for the MSVC linker.

## Alternatives considered

- **One crate per service with modules per layer** (zero-to-production, howtocodeit): simpler, but a layer violation is an import a tool must find instead of an edge the compiler refuses.
- **just or Make**: a second tool and shell dialects; logic that cannot be unit tested.
- **A nightly toolchain for custom lints (dylint)**: type-aware rules, but the whole workspace would have to build on that nightly. Deferred.
