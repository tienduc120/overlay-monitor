//! Adapters for this service's ports. Each one turns its storage's conditions into the application's errors.

mod memory;

pub use memory::MemoryNoteRepository;
