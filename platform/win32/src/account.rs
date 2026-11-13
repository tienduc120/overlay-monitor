//! The signed-in account, as Windows names it.

use windows::Win32::Foundation::{CloseHandle, E_INVALIDARG, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::PWSTR;

use crate::Win32Error;

/// The security identifier of the user this process runs as, such as `S-1-5-21-...-1001`.
///
/// # Errors
/// The process token could not be opened or read, or the identifier could not be written as text.
pub fn current_user_sid() -> Result<String, Win32Error> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo handle of the current process needs no closing, and `token` is a valid out pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) }?;
    let sid = token_user_sid(token);
    // SAFETY: `token` was opened above, is owned here, and is not used again.
    unsafe { CloseHandle(token) }.ok();
    sid
}

fn token_user_sid(token: HANDLE) -> Result<String, Win32Error> {
    let mut needed = 0_u32;
    // SAFETY: no buffer and a length of zero only ask for the size, which is written to `needed`. The call is
    // expected to fail with "insufficient buffer", so its result is not an error here.
    unsafe { GetTokenInformation(token, TokenUser, None, 0, &raw mut needed) }.ok();
    if needed == 0 {
        return Err(Win32Error::from_thread());
    }
    // u64 elements keep the buffer aligned for TOKEN_USER.
    let mut buffer = vec![0_u64; usize::try_from(needed).unwrap_or(0).div_ceil(8)];
    // SAFETY: `buffer` holds at least `needed` bytes and outlives the call.
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
        )
    }?;
    // SAFETY: on success the buffer starts with a TOKEN_USER, and it is aligned as the type requires.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = PWSTR::null();
    // SAFETY: `user.User.Sid` points into `buffer`, which is alive; the call allocates the string `text` receives.
    unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) }?;
    // SAFETY: on success `text` is a NUL-terminated string valid until it is freed below.
    let sid = unsafe { text.to_string() };
    // SAFETY: the string was allocated by `LocalAlloc` and is freed exactly once, after its last use.
    unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    sid.map_err(|_not_utf16| Win32Error::from(E_INVALIDARG))
}

#[cfg(test)]
mod tests;
