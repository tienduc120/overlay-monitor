use std::error::Error;
use std::future::Future;

use example_domain::Note;

/// A port: the application declares what it needs; infrastructure implements it. The future is `Send` so a use case
/// can run on any runtime thread.
pub trait NoteRepository: Send + Sync {
    /// Saves a new value. A taken id is [`SaveError::AlreadyExists`], whatever the storage.
    fn save(&self, note: &Note) -> impl Future<Output = Result<(), SaveError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("this id is taken")]
    AlreadyExists,
    #[error("storage failed")]
    Storage(#[source] Box<dyn Error + Send + Sync>),
}
