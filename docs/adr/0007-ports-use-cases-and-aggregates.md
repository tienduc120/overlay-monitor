# 0007 - Ports, use cases, and aggregates

- **Status**: accepted
- **Date**: 2026-11-12

## Context

Async Rust has no `async fn` in `dyn` traits on stable, and a plain `async fn` in a public trait cannot promise `Send`. Aggregates need one way in that checks their invariants and one way back from storage that does not.

## Decision

1. **A port is a trait declared in `application`** whose async methods are written `fn x(&self, ..) -> impl Future<Output = ..> + Send`. An adapter may implement it with `async fn`, or return `std::future::ready(..)` when it has nothing to await.
2. **Use cases are generic over their ports**, not `dyn`. The composition root picks the adapters. `async-trait` is banned in `deny.toml`.
3. **A use case takes a command of plain values and returns its own result and error types**, so delivery never handles a domain type.
4. **An aggregate root is marked with `agent:aggregate` in its doc comment.** A source rule requires a field `id` whose type the crate defines (`NoteId`), and `new` and `restore`. Private fields make a struct literal outside the domain a compile error.
5. **Only infrastructure calls `restore`**, outside tests, checked by a source rule. A value rebuilt without its checks can break an invariant the rest of the code relies on.
6. **Fakes of a crate's ports live in its `testing` module** behind a `test-support` feature, enabled only as a dev-dependency, so other layers' tests use them without naming types they may not depend on.

## Consequences

- Positive: `Send` is checked where the port is declared; static dispatch; an aggregate cannot be built around its checks.
- Negative: generic use cases make the composition root spell out types; a runtime choice of adapter means calling a generic function twice, not a `Box<dyn ..>`.
- Neutral: the `restore` rule goes by name, so another type's `restore` in the application would be reported too.

## Alternatives considered

- **`async-trait` or `dyn` with boxed futures**: an allocation per call and a second way to write the same trait.
- **`trait_variant`**: a macro for what one explicit signature says.
- **Recognising aggregates by shape**: a wrong guess would block valid code.
