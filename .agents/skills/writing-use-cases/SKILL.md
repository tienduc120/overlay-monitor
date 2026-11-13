---
name: writing-use-cases
description: Use when adding or changing a use case or a port in a service's application layer, especially one that writes to storage, calls another service, or publishes an event. Follows ADR-0007.
---

# Write a use case

A use case is one operation the service offers, in `application`. It orchestrates: rules live in the domain, storage behind a port. `services/example/application/src/create_note.rs` is the shape to copy.

## Shape

1. **Command in, result and error out.** The command and result are plain values (`String`, numbers), so delivery never builds or reads a domain type. The error is the use case's own enum (skill: `handling-errors`).
2. **Validate by building domain values** with their constructors (`NoteId::new`, `Note::new`); a failure becomes the `Invalid` outcome through `#[from]`.
3. **Declare what you need as a port** in `ports.rs`: a trait with `Send + Sync` and methods written `fn save(&self, ..) -> impl Future<Output = Result<.., ..>> + Send`. Name a repository port `…Repository`.
4. **Take time from the clock port**: the use case is generic over `C: platform_clock::Clock`.
5. **Be generic over the ports** (`CreateNote<R, C>`); the composition root chooses the adapters.
6. **Add a fake** of each new port to the crate's `testing` module, so this crate's tests and other layers' tests share one.

## Effects and retries

- The caller chooses the new id, so a retried request meets `AlreadyExists` instead of creating a second copy.
- An effect that cannot be rolled back (a call to another service, an email, a message) happens before the write that depends on it, or after it commits, never between two writes that must agree. A failure after the commit is logged and retried, not returned as the use case's failure.

## Testing

Test it over the fakes with a `FixedClock`. **Done** when every outcome has a test that went red, and each test asserts what the fake recorded, including that nothing was saved when the command was invalid.
