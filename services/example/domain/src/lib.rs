//! This service's domain: the rules that keep its values valid. `no_std` keeps I/O, threads, and the clock out; time
//! arrives as a value (ADR-0006).
#![no_std]

extern crate alloc;

mod note;

pub use note::{Note, NoteError, NoteId};
