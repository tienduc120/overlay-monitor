//! Hand-written fakes of this crate's ports, for this service's tests (feature `test-support`).

use std::future::{Future, ready};
use std::sync::{Mutex, PoisonError};

use example_domain::Note;

use crate::{NoteRepository, SaveError};

#[derive(Debug, Clone, Copy)]
pub enum SaveOutcome {
    Saved,
    Taken,
    Broken,
}

/// Records every save and answers with the programmed outcome.
#[derive(Debug)]
pub struct FakeNoteRepository {
    saved: Mutex<Vec<Note>>,
    outcome: SaveOutcome,
}

impl FakeNoteRepository {
    pub const fn new(outcome: SaveOutcome) -> Self {
        Self {
            saved: Mutex::new(Vec::new()),
            outcome,
        }
    }

    pub fn saved(&self) -> Vec<Note> {
        self.saved.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl NoteRepository for FakeNoteRepository {
    fn save(&self, note: &Note) -> impl Future<Output = Result<(), SaveError>> + Send {
        self.saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(note.clone());
        ready(match self.outcome {
            SaveOutcome::Saved => Ok(()),
            SaveOutcome::Taken => Err(SaveError::AlreadyExists),
            SaveOutcome::Broken => Err(SaveError::Storage("disk full at /var/lib/notes".into())),
        })
    }
}
