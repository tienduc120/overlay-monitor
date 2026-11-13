# 0002 - Gates, proven by fixtures

- **Status**: accepted
- **Date**: 2026-11-12

## Context

Agents write most of the code, so a rule that depends on being remembered will be broken. Each rule needs a tool that fails the build, and each tool needs proof that it still fires: a lint level can be loosened by accident, and a config file that clippy cannot find makes every rule in it silent.

## Decision

1. **One command runs the gates.** `cargo xtask verify` runs rustfmt, xtask's own gates (layer rules, manifest policy, source rules), strict clippy, nextest, and typos. `--full` adds cargo-deny, cargo-shear, rustdoc with warnings denied, and the rules proof. Every step runs even after a failure; the full output goes to `.xtask/logs` and only the tail of a failure comes back. Warnings fail the build through `CARGO_BUILD_WARNINGS=deny`, which does not invalidate the cache the way `RUSTFLAGS` does.
2. **Rules are proven, not assumed.** `cargo xtask rules` stages every fixture in `fixtures/violations/<id>/` and the clean twin in `fixtures/clean/` as one throwaway workspace with the repository's real lint table and configuration files, runs clippy once and xtask's gates once, and checks that each fixture's own enforcer reports the text its `rule.toml` names under its own directory. The clean twin passes only when nothing reports anything.
3. **xtask's own gates cover what Cargo and clippy cannot say**: the layer rules in `architecture.toml` checked against `cargo metadata`, the manifest policy, and source rules read with `syn`.
4. **CI runs `verify --full`** on the pinned toolchain and tools, checks every commit message of a pull request, fails if a gate left the worktree dirty, and audits the workflows with zizmor. Actions are pinned to a commit SHA.

## Consequences

- Positive: a weakened rule fails `rules` by name (checked by setting `unwrap_used` to allow: the `unwrap-used` fixture failed). A new rule cannot land without a fixture.
- Negative: every rule costs a fixture crate, and the staged workspace compiles its dependencies once per cache.
- Neutral: clippy rules are proven on the lints this repository configures, not on clippy's defaults.

## Alternatives considered

- **Trust the configuration**: it erodes silently.
- **trybuild or ui_test** for every rule: right for compile errors and custom lints, heavier than a staged workspace for configuration that already exists.
- **cargo-pup** (DataDog) for layer rules: needs a pinned nightly and is young; the dependency graph from `cargo metadata` says the same on stable.
