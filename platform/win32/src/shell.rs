//! Things the Windows shell does for the app.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, HWND};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HRESULT, PCWSTR, w};

use crate::Win32Error;

/// `ShellExecuteW` answers a value above this when it worked, and an error code up to it when it did not.
const LAST_ERROR_CODE: usize = 32;

/// Opens `url` in the user's default browser.
///
/// # Errors
/// The shell refused: no program is set for the address, or it could not be started.
pub fn open_url(url: &str) -> Result<(), Win32Error> {
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    // SAFETY: the verb is a static string and `wide` is a null-terminated copy that outlives the call; no owner
    // window, parameters, or directory are given.
    let result = unsafe {
        ShellExecuteW(
            None::<HWND>,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0.addr();
    if code > LAST_ERROR_CODE {
        return Ok(());
    }
    let code = u32::try_from(code).unwrap_or(ERROR_FILE_NOT_FOUND.0);
    Err(Win32Error::from_hresult(HRESULT::from_win32(code)))
}
