# 0014 - Sync ports and scoped threads

- **Status**: accepted
- **Date**: 2026-11-13

## Context

ADR-0007 makes ports async traits and ADR-0009 runs work as supervised tasks on `platform-task`. Overlay Monitor is a desktop app with no server: its sources (counters, ETW, a driver) are blocking calls, and an async runtime adds a dependency and a model it does not need. Approved by the owner on 2026-11-13.

## Decision

1. **This app's ports are plain `fn`s**, not functions returning futures.
2. **Sampling and ETW consumption run on `std::thread::scope` threads**, not on `platform-task`; no async runtime is a dependency.
3. **This departs from ADR-0007's async ports and ADR-0009's supervised tasks for this desktop app only**; the base's rules for services that need them stand. ADR-0007's index status becomes `superseded in part by 0014`.
4. **Threads are started only through `std::thread::scope`**, so a thread cannot outlive the data it borrows and shutdown joins them.

## Consequences

- Positive: no runtime, simpler ports, tests call ports directly.
- Negative: no cancellation token or supervision from `platform-task`; each loop checks a stop flag itself.
- Neutral: the clippy bans on `std::thread::spawn` stay.

## Alternatives considered

- **Keep async ports on tokio**: a runtime and `Send` bounds for blocking calls.
- **`std::thread::spawn` with join handles**: threads may outlive borrowed state; refused by clippy.
