use alloc::string::String;

use jiff::Timestamp;

/// Ids are typed, so one aggregate's id cannot be passed where another's is expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteId(String);

impl NoteId {
    pub fn new(raw: &str) -> Result<Self, NoteError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(NoteError::EmptyId);
        }
        Ok(Self(String::from(raw)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NoteError {
    #[error("the id must not be empty")]
    EmptyId,
    #[error("the name must not be empty")]
    EmptyName,
}

/// The aggregate root this service owns. agent:aggregate
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    id: NoteId,
    name: String,
    created_at: Timestamp,
}

impl Note {
    /// The only way to make a new value: it checks every rule. The time is a value; the domain never reads the clock.
    pub fn new(id: NoteId, name: &str, created_at: Timestamp) -> Result<Self, NoteError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(NoteError::EmptyName);
        }
        Ok(Self {
            id,
            name: String::from(name),
            created_at,
        })
    }

    /// Rebuilds a stored value for a repository. It was valid when saved, so nothing is checked again; only
    /// infrastructure calls this (`cargo xtask verify` enforces it).
    pub const fn restore(id: NoteId, name: String, created_at: Timestamp) -> Self {
        Self { id, name, created_at }
    }

    pub const fn id(&self) -> &NoteId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

#[cfg(test)]
mod tests;
