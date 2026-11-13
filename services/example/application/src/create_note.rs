use example_domain::{Note, NoteError, NoteId};
use platform_clock::Clock;

use crate::{NoteRepository, SaveError};

/// Plain values in, so a caller never builds a domain type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateNoteCommand {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateNoteResult {
    pub id: String,
}

/// Every outcome a caller can branch on; `interfaces` maps each one to a status.
#[derive(Debug, thiserror::Error)]
pub enum CreateNoteError {
    #[error("invalid command: {0}")]
    Invalid(#[from] NoteError),
    #[error("{0} already exists")]
    AlreadyExists(String),
    #[error("could not save")]
    Storage(#[source] SaveError),
}

pub struct CreateNote<R, C> {
    repo: R,
    clock: C,
}

impl<R: NoteRepository, C: Clock> CreateNote<R, C> {
    pub const fn new(repo: R, clock: C) -> Self {
        Self { repo, clock }
    }

    /// The caller chooses the id, so a retried request meets `AlreadyExists` instead of creating a second copy.
    pub async fn execute(&self, cmd: CreateNoteCommand) -> Result<CreateNoteResult, CreateNoteError> {
        let note = Note::new(NoteId::new(&cmd.id)?, &cmd.name, self.clock.now())?;
        let id = note.id().as_str().to_owned();
        match self.repo.save(&note).await {
            Ok(()) => Ok(CreateNoteResult { id }),
            Err(SaveError::AlreadyExists) => Err(CreateNoteError::AlreadyExists(id)),
            Err(e @ SaveError::Storage(_)) => Err(CreateNoteError::Storage(e)),
        }
    }

    #[cfg(test)]
    const fn repository(&self) -> &R {
        &self.repo
    }
}

#[cfg(test)]
mod tests;
