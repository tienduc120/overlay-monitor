---
name: handling-errors
description: Use when a function can fail, when adding a use-case outcome or an HTTP status mapping, or when an error crosses a layer. Follows ADR-0011.
---

# Handle errors

Failure is a returned `Result`, and each layer owns the errors it raises. `services/monitor` shows the shape: `application/src/ports.rs` declares `ReadError`, the adapters in `infrastructure/src` return it, and `api/src/lib.rs` is where an outcome ends.

The names in the examples below (`Note`, `NoteError`, `CreateNote`, `CreateNoteError`, `SaveError`) and the HTTP statuses are illustrative: they are not in this repository, which has no HTTP delivery. `ReadError` is the real port error to read; map an outcome to whatever the delivery layer shows.

## What each layer does

| Layer | Declares | Does with an error |
| --- | --- | --- |
| domain | a `thiserror` enum of broken invariants (`NoteError::EmptyName`) | returns it from `new` |
| application | per use case, an enum of the outcomes a caller branches on (`CreateNoteError`); per port, the conditions its callers branch on (`SaveError::AlreadyExists`) | wraps a domain error with `#[from]`; turns a port's condition into an outcome; keeps the rest as `#[source]` |
| infrastructure | nothing other layers need | turns its storage's condition into the port's error (a unique-key violation becomes `SaveError::AlreadyExists`) |
| interfaces | nothing | matches every outcome to a status; logs the cause of an internal one |

## Rules

1. The cause travels: `#[source]` or `#[from]` on the variant that wraps it. A message names the operation that failed ("save note"), and the source chain says why.
2. Absence is the use case's call: a repository lookup answers `Option<T>`, and the use case decides whether `None` is an outcome.
3. Only `interfaces` picks a status, with an exhaustive `match` and no wildcard arm: invalid input 400, unauthenticated 401, forbidden 403, absent 404, a taken id or a state conflict 409, anything internal 500 with the body `internal error` and the cause in the log.
4. Name an error enum after the operation (`CreateNoteError`) and a variant after the condition (`AlreadyExists`).

## Adding an outcome

1. Write the handler test first: the use case returns the new outcome, expect its status and a body that leaks nothing internal. Watch it go red.
2. Add the variant to the use case's error enum and return it.
3. The handler's `match` stops compiling until you add the arm: give it the status.
4. **Done** when you can walk one failing path from where it starts to the response and point at each layer's translation.
