//! Named mutexes shared with other hardware monitors, which agree on these names to keep from touching the same
//! bus at once.

use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{CreateMutexW, INFINITE, ReleaseMutex, WaitForSingleObject};
use windows::core::PCWSTR;

use crate::Win32Error;

/// Entry point for taking a named system mutex.
#[derive(Debug)]
pub struct GlobalMutex;

/// Holds the mutex until dropped. It stays on the thread that took it, as the mutex's owner must release it.
#[derive(Debug)]
pub struct GlobalMutexGuard {
    handle: HANDLE,
}

impl GlobalMutex {
    /// Takes the mutex `name` (given in full, `Global\...` or `Local\...`), waiting at most `timeout`.
    /// `Ok(None)` means another holder kept it past the timeout.
    ///
    /// # Errors
    /// The system refused to create or wait on the mutex.
    pub fn acquire(name: &str, timeout: Duration) -> Result<Option<GlobalMutexGuard>, Win32Error> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // One below INFINITE, so a huge timeout still ends.
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX).min(INFINITE - 1);
        // SAFETY: `wide` is NUL-terminated and outlives the call; no security attributes are passed.
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr())) }?;
        // SAFETY: `handle` was just created and is owned by this function.
        let waited = unsafe { WaitForSingleObject(handle, millis) };
        // An abandoned mutex was left by a holder that died; it is still ours now.
        if waited == WAIT_OBJECT_0 || waited == WAIT_ABANDONED {
            return Ok(Some(GlobalMutexGuard { handle }));
        }
        // SAFETY: the handle is ours and is not used again.
        unsafe { CloseHandle(handle) }.ok();
        if waited == WAIT_TIMEOUT {
            Ok(None)
        } else {
            Err(Win32Error::from_thread())
        }
    }
}

impl Drop for GlobalMutexGuard {
    fn drop(&mut self) {
        // SAFETY: the guard owns the handle and the owning thread drops it; neither call is repeated.
        unsafe {
            ReleaseMutex(self.handle).ok();
            CloseHandle(self.handle).ok();
        }
    }
}

#[cfg(test)]
mod tests;
