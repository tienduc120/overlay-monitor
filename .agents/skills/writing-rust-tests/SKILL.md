---
name: writing-rust-tests
description: Use before writing production code, and when writing or changing a test in this repository. The red-green loop with nextest, where a test lives, and what each layer tests with.
---

# Write Rust tests

A test that never went red proves nothing; a test written after the code only describes it.

## The loop

1. **Red.** Write one test for one behaviour, run `cargo nextest run -p <crate>`, and watch it fail because the behaviour is missing. A compile error on a missing function counts only once a stub exists; a stub returning a wrong value makes the red honest.
2. **Green.** Write the least code that passes. Run the crate's tests again; all of them pass.
3. **Done** when every new branch of logic has a test you watched go red, and the crate's whole suite is green.

An existing test is the specification: change it only when the specification changes, and say so in the commit. A `refactor:` commit only adds tests.

## Where a test lives

- A unit test sits in a sibling file: `#[cfg(test)] mod tests;` at the end of `foo.rs`, the tests in `foo/tests.rs` (`lib.rs` pairs with `src/tests.rs`). The source rules refuse an inline test module.
- Name a test for the behaviour, as a sentence: `a_taken_id_is_a_conflict_not_a_storage_failure`. Arrange, act, assert, as three blocks.
- Compare whole values with `assert_eq!` and say what a failure means in the message. `unwrap()` and indexing are fine in tests.

## What each layer tests with

| Layer | Test with |
| --- | --- |
| domain | plain values; time as a `jiff::Timestamp` you build |
| application | the fakes in its own `testing` module and `platform_clock::FixedClock` |
| infrastructure | the adapter itself, with a real backing store when there is one |
| interfaces | the use case built over the application's fakes (`features = ["test-support"]`) |
| api | the assembled app with fakes for the ports, stopped through its stop flag |

A fake records what it was asked and answers with a programmed outcome; assert on what it recorded.

## Time and concurrency

- Wall time comes from the clock port, so a test fixes it with `FixedClock`.
- A test waits on a timer, a channel, or an event, never on `std::thread::sleep`.
- Threads in a test start through `std::thread::scope`, so nothing outlives the test.

## Before you finish

State which tests you ran and what they printed. Name any you skipped and why.
