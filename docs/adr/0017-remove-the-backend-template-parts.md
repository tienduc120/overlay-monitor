# 0017 - Remove the backend template parts

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The base this app was copied from builds HTTP services. Overlay Monitor is a desktop app: ADR-0014 already dropped async ports and supervised tasks, and nothing here serves HTTP, runs a tokio runtime, or starts another service. The parts left over are code and rules that judge nothing, and the owner asked for them removed at the end of the first plan. Approved by the owner on 2026-11-13, who also asked for the owner's icon on the exe file.

## Decision

1. **These crates are removed**: `services/example` (the witness), `platform/http`, and `platform/task`. `platform-clock` and `platform-win32` stay.
2. **The service generator is removed**: `cargo xtask new-service`, its code and tests, and the `new-service` skill. This supersedes ADR-0010. A future service is written by hand, or the generator returns through a new record.
3. **The supervised-task and HTTP-server parts of ADR-0009 are superseded.** Its clock port and its rule that process-wide state is set in the composition root stand.
4. **These workspace dependencies are removed**: `platform-http`, `platform-task`, `axum`, `tokio`, `tokio-util`, `tower`, `tower-http`, and `http-body-util`. `serde` and `serde_json` stay for xtask.
5. **These clippy bans are removed**, because they name crates no longer in the workspace: every `tokio::*` spawn method, `tokio_util::sync::CancellationToken::new`, `axum::serve`, and the `await-holding-invalid-types` block for tokio guards. The bans on `std::thread::spawn`, `std::thread::Builder::spawn`, `std::thread::sleep`, the clock reads, `std::env`, `std::process::exit`, and the global `tracing` subscriber stay, with their reasons pointing at `std::thread::scope` and at waiting on a timer, a channel, or an event.
6. **These fixtures are removed**: `axum-serve`, `root-token`, and `lock-across-await`. The `spawn` fixture now proves the `std::thread::spawn` ban, and `cargo xtask rules` must stay green.
7. **`architecture.toml` keeps `axum`, `tower-http`, and `hyper` in the infrastructure deny list.** No gate or fixture needs them, but denying a web stack in a desktop app costs nothing and keeps one from arriving unnoticed.
8. **`embed-resource` 3.0.12 (MIT) is a build dependency of `monitor-api`**, pinned in the root `Cargo.toml`. `build.rs` compiles `app.rc`, which holds only the icon (`1 ICON "assets/icon.ico"`), so the exe file shows the owner's icon. The manifest still comes from the linker arguments in `build.rs`, and `app.rc` carries none, so the two do not collide.
9. **`docs/deferred.md` drops the backend-only items**: the Postgres adapter, the transaction boundary, connection limits, the worker template, and the public API boundary.

## Consequences

- Positive: nothing in the repository describes or checks a part the app does not have; the dependency graph is smaller.
- Negative: a later service needing a runtime or a server must bring back a platform crate and its bans through a new record. One more build dependency (`embed-resource` and its build-time helpers).
- Neutral: the xtask tests keep synthetic crate names such as `platform-http` as sample data; they test the gate, not the crate.

## Alternatives considered

- **Keep the parts as unused template**: they still compile, are linted, and are read by every agent, for nothing.
- **Embed the icon with a hand-written `windres` or `rc` call in `build.rs`**: more code and a toolchain assumption that `embed-resource` already handles.
- **Drop the three denied web crates from `architecture.toml`**: loosens a rule for no gain.
