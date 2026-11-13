# 0009 - Supervised tasks, the clock port, and the HTTP server

- **Status**: superseded in part by 0017
- **Date**: 2026-11-12

## Context

A spawned task nobody awaits is a leak, and its panic is lost: tokio catches a task's panic instead of crashing the process. A server with no timeouts is held open by its slowest client, and axum's graceful shutdown has no deadline. A clock read inside logic makes it untestable.

## Decision

1. **`platform-task` is the only place a task is started.** `Tasks` runs named tasks with a child cancellation token; the first failure or panic cancels the rest; `wait` returns every failure, a panic becoming an error that names the task; dropping the set aborts whatever still runs. Once cancelled, tasks get a grace period with no default, then are aborted and reported as stuck, so a task that ignores its token cannot hang shutdown. clippy refuses the spawn family, `std::thread::spawn`, and `std::thread::sleep` everywhere else.
2. **Cancellation is derived, not created.** `CancellationToken::new` is refused outside a composition root; everything else takes `child_token()` of the token it was given.
3. **`platform-http::Server` is the only way to serve.** `Limits` has no default and refuses zero; every request gets a timeout (503), a body limit (413), and a panic guard (500); shutdown drains in-flight requests within its own deadline. `axum::serve` is refused elsewhere.
4. **`platform-clock` is the time port.** `SystemClock` is the only clock read; tests use `FixedClock`.
5. **Process-wide state is set in the composition root only**: the global `tracing` subscriber, configuration from the environment, the root token, and the exit code.

## Consequences

- Positive: one obvious way to run concurrent work, serve, and read time, each tested once.
- Negative: there is no Rust equivalent of goleak; structured tasks and the spawn ban are the guarantee instead. Blocking work needs a helper added to `platform-task` when first needed.
- Neutral: `tokio::time::pause` controls tokio timers in tests; the clock port controls wall time.

## Alternatives considered

- **`tokio_util::task::TaskTracker`**: tracks tasks but returns no results or panics.
- **Timeouts per handler**: every author must remember them.
