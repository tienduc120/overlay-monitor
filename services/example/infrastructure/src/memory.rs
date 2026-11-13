use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::sync::{Mutex, PoisonError};

use example_application::{NoteRepository, SaveError};
use example_domain::{Note, NoteId};

/// Keeps every value in process memory: for tests and a first local run.
#[derive(Debug, Default)]
pub struct MemoryNoteRepository {
    notes: Mutex<BTreeMap<NoteId, Note>>,
}

impl NoteRepository for MemoryNoteRepository {
    /// Nothing to wait for, so the answer is ready at once.
    fn save(&self, note: &Note) -> impl Future<Output = Result<(), SaveError>> + Send {
        // A panic while holding the lock cannot leave the map half-written, so a poisoned lock is still sound.
        let mut notes = self.notes.lock().unwrap_or_else(PoisonError::into_inner);
        let saved = if notes.contains_key(note.id()) {
            Err(SaveError::AlreadyExists)
        } else {
            notes.insert(note.id().clone(), note.clone());
            Ok(())
        };
        ready(saved)
    }
}

#[cfg(test)]
mod tests;
