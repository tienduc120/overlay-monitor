//! One running copy per session, held by a named mutex.

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use crate::Win32Error;

/// Holds the name until dropped, so a second copy finds it taken.
#[derive(Debug)]
pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    /// Takes the name (given in full, `Local\...` for this session). `Ok(None)` means another copy holds it.
    ///
    /// # Errors
    /// The system refused to create the mutex.
    pub fn acquire(name: &str) -> Result<Option<Self>, Win32Error> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call; no security attributes are passed, and the mutex is
        // not taken, only named.
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr())) }?;
        // CreateMutexW succeeds for a name that exists and says so only here, so read it before anything else can
        // overwrite it.
        // SAFETY: reading the thread's last error has no preconditions.
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if existed {
            // SAFETY: the handle is ours and is not used again.
            unsafe { CloseHandle(handle) }.ok();
            return Ok(None);
        }
        Ok(Some(Self { handle }))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: the handle is owned here and closed once.
        unsafe { CloseHandle(self.handle) }.ok();
    }
}

#[cfg(test)]
mod tests;
