# Deferred work

Left out on purpose until a real project needs it. Each entry says what triggers it and what must be true before it counts as done. Do not start one without its trigger, and do not report it as enforced until a tool in this repository checks it. Every item follows the repository's own rules: an ADR first, tests first, a fixture for every new rule, a credit in the README for any public source, and a pinned tool in `cargo xtask doctor`.

| Item | Trigger | Done when |
| --- | --- | --- |
| Postgres adapter and query gates | The first service that stores data | sqlx 0.9 with queries in `.sql` files and the committed `.sqlx` offline cache (`cargo sqlx prepare --check` in verify); `platform-postgres` opens every pool with `statement_timeout`, `lock_timeout`, and `idle_in_transaction_session_timeout`; a per-request query budget (a `tracing` layer counting `sqlx::query` events) fails a test on an N+1; a query linter (sqlparser) refuses `SELECT *`, an unbounded list, and `OFFSET`; `EXPLAIN` on a throwaway Postgres refuses a sequential scan; squawk lints migrations; a fixture per rule |
| Transaction boundary | A use case that writes more than one row | A `Transactor` port in `application`; no network call inside the closure, checked by a source rule with a fixture; an outbox written in the same transaction |
| Bounded command runner | An agent session hangs on a command the guard does not know | `cargo xtask run -- <cmd>` with a deadline, an idle limit, a process-tree kill, and the output in `.xtask/logs`; the guard points hanging commands at it |
| Debt register | The first `#[expect]` that is not a permanent design choice | `cargo xtask debt` lists every `#[expect]`, untracked TODO, and function near the size limits with a stable id; decisions with an expiry live in `docs/debt/register.json`; `verify` fails on new undecided debt |
| Perf loop | The first performance problem | An ADR on the tools (pprof-rs and jemalloc heap profiles are Linux only; samply works everywhere); a loopback-only debug listener in `platform-http`; a load program per service |
| Worker template | The first service with no API | `new-service --kind worker`; a witness that passes every gate |
| ADR and Markdown checks | The ADR index or the Markdown drifts | `cargo xtask` checks ADR numbering, sections, and the index, and one paragraph per line in Markdown |
| Connection limits | The first service exposed to clients you do not control | `platform-http` serves through hyper-util with a bound on idle keep-alive connections and on the time to stream a response body, each with a test; today only the handler time, the request body, the header read (hyper's 30 seconds), and the shutdown drain are bounded |
| Public API boundary | A use case returns a domain type to `interfaces` | A check that an application crate's public API exposes no domain type (cargo-check-external-types or a source rule) |
| Toolchain 1.100 | Rust 1.100 reaches the stable channel | `rust-toolchain.toml`, `rust-version`, and `doctor` move together; `[lints.cargo] unused_dependencies` replaces cargo-shear if it proves equal |
