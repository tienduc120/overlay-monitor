//! What the two `PawnIO` sensors share: loading a module file and taking a system lock.

use std::io;
use std::path::Path;
use std::time::Duration;

use monitor_application::ReadError;
use platform_win32::Win32Error;
use platform_win32::mutex::GlobalMutex;
use platform_win32::pawnio::{PawnError, PawnModule};

/// Other hardware monitors hold these locks too, so waiting longer would stall the overlay for nothing.
const LOCK_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, thiserror::Error)]
enum LoadError {
    #[error("read module file {file}")]
    ReadFile {
        file: String,
        #[source]
        source: io::Error,
    },
    #[error("load module {file}")]
    Load {
        file: String,
        #[source]
        source: PawnError,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum LockError {
    #[error("lock {name}")]
    Acquire {
        name: String,
        #[source]
        source: Win32Error,
    },
    #[error("lock {name} stayed held by another program")]
    Busy { name: String },
}

/// Loads `file` from `modules_dir`. What the machine lacks is `Unavailable`; anything else is `Failed`.
pub(crate) fn load_module(modules_dir: &Path, file: &str) -> Result<PawnModule, ReadError> {
    let blob = std::fs::read(modules_dir.join(file)).map_err(|source| {
        let reason = format!("module file {file} is missing: run `cargo xtask modules`");
        if source.kind() == io::ErrorKind::NotFound {
            ReadError::Unavailable(reason)
        } else {
            ReadError::Failed(Box::new(LoadError::ReadFile {
                file: file.to_owned(),
                source,
            }))
        }
    })?;
    PawnModule::load(&blob).map_err(|source| open_failure(source, file))
}

/// What the machine lacks is `Unavailable` for good; anything else is a failure with its cause kept.
fn open_failure(source: PawnError, file: &str) -> ReadError {
    match source {
        PawnError::NotInstalled | PawnError::AccessDenied | PawnError::NotSupported => {
            ReadError::Unavailable(source.to_string())
        }
        PawnError::NameTooLong | PawnError::Io(_) => ReadError::Failed(Box::new(LoadError::Load {
            file: file.to_owned(),
            source,
        })),
    }
}

/// Runs `body` while holding the system mutex `name`; a lock that is not free in time is a failed read.
pub(crate) fn with_lock<T>(name: &str, body: impl FnOnce() -> Result<T, ReadError>) -> Result<T, ReadError> {
    let guard = GlobalMutex::acquire(name, LOCK_TIMEOUT).map_err(|source| {
        ReadError::Failed(Box::new(LockError::Acquire {
            name: name.to_owned(),
            source,
        }))
    })?;
    let Some(_guard) = guard else {
        return Err(ReadError::Failed(Box::new(LockError::Busy { name: name.to_owned() })));
    };
    body()
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CallError {
    #[error("call {function}")]
    Call {
        function: &'static str,
        #[source]
        source: PawnError,
    },
    #[error("{function} returned no value")]
    Empty { function: &'static str },
    #[error("{function} returned {value:#x}, which does not fit the register")]
    TooWide { function: &'static str, value: u64 },
}

/// Calls a module function that returns one register-sized word.
pub(crate) fn call_one(module: &PawnModule, function: &'static str, input: &[u64]) -> Result<u64, ReadError> {
    let words = module
        .execute(function, input, 1)
        .map_err(|source| ReadError::Failed(Box::new(CallError::Call { function, source })))?;
    words
        .first()
        .copied()
        .ok_or_else(|| ReadError::Failed(Box::new(CallError::Empty { function })))
}

/// Narrows a reply to the register's width, refusing a value with stray high bits.
pub(crate) fn narrow<T: TryFrom<u64>>(function: &'static str, value: u64) -> Result<T, ReadError> {
    T::try_from(value).map_err(|_wide| ReadError::Failed(Box::new(CallError::TooWide { function, value })))
}

#[cfg(test)]
mod tests;
