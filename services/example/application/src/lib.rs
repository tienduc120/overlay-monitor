//! This service's use cases, and the ports their adapters implement (ADR-0007). A use case takes a command of plain
//! values and returns its own result and error types, so delivery never handles a domain type.

mod create_note;
mod ports;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use create_note::{CreateNote, CreateNoteCommand, CreateNoteError, CreateNoteResult};
pub use ports::{NoteRepository, SaveError};
