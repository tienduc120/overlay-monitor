# Architecture decisions

One file per decision, numbered, never edited after acceptance (supersede it with a new record instead).

| # | Title | Status |
| --- | --- | --- |
| [0001](0001-foundations.md) | Foundations | accepted |
| [0002](0002-gates-proven-by-fixtures.md) | Gates, proven by fixtures | accepted |
| [0003](0003-git-hooks.md) | Git hooks | accepted |
| [0004](0004-lint-policy.md) | Lint policy: deny everything, suppress one item with a reason | accepted |
| [0005](0005-agent-guard.md) | Agent guard | accepted |
| [0006](0006-the-domain-is-no-std.md) | The domain is no_std | accepted |
| [0007](0007-ports-use-cases-and-aggregates.md) | Ports, use cases, and aggregates | superseded in part by 0014 |
| [0008](0008-supply-chain.md) | Supply chain | accepted |
| [0009](0009-supervised-tasks-clock-and-http.md) | Supervised tasks, the clock port, and the HTTP server | superseded in part by 0017 |
| [0010](0010-the-service-generator-copies-the-witness.md) | The service generator copies the witness | superseded by 0017 |
| [0011](0011-errors-are-values-owned-by-the-layer-that-raises-them.md) | Errors are values owned by the layer that raises them | accepted |
| [0012](0012-unsafe-only-in-platform-win32.md) | Unsafe code lives only in platform-win32 | accepted |
| [0013](0013-the-windows-crate.md) | The windows crate | accepted |
| [0014](0014-sync-ports-and-scoped-threads.md) | Sync ports and scoped threads | accepted |
| [0015](0015-ci-runs-on-windows.md) | CI runs on Windows | accepted |
| [0016](0016-overlay-presentation.md) | Overlay presentation | accepted |
| [0017](0017-remove-the-backend-template-parts.md) | Remove the backend template parts | accepted |

Needs an ADR first: a new external dependency, a hosted service, the toolchain pin or a pinned tool moving, a change to a published contract, or a gate added, loosened, or removed.
