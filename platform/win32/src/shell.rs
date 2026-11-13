//! Things the Windows shell does for the app.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, HWND};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW, SW_SHOWNORMAL};
use windows::core::{HRESULT, PCWSTR, w};

use crate::Win32Error;

/// `ShellExecuteW` answers a value above this when it worked, and an error code up to it when it did not.
const LAST_ERROR_CODE: usize = 32;

/// Opens `url` in the user's default browser, at the user's own level even from the elevated app.
///
/// The address goes to `explorer.exe`, which hands it to the desktop's running Explorer: a browser started straight
/// from this process would run as administrator, which browsers warn about or refuse to hand to their open window.
///
/// # Errors
/// The shell refused: `explorer.exe` could not be started.
pub fn open_url(url: &str) -> Result<(), Win32Error> {
    let quoted: Vec<u16> = format!("\"{url}\"").encode_utf16().chain(Some(0)).collect();
    // SAFETY: the verb and the file are static strings and `quoted` is a null-terminated copy that outlives the call;
    // no owner window or directory is given.
    let result = unsafe {
        ShellExecuteW(
            None::<HWND>,
            w!("open"),
            w!("explorer.exe"),
            PCWSTR(quoted.as_ptr()),
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

/// Shows `text` in an error box and waits for it to be closed. The release exe has no console, so this is how a
/// startup failure reaches the user.
pub fn show_error(title: &str, text: &str) {
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: both strings are null-terminated copies that outlive the call, and there is no owner window.
    // The answer is always `OK`, so it is not read.
    unsafe {
        MessageBoxW(
            None::<HWND>,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_ICONERROR | MB_OK | MB_SETFOREGROUND,
        );
    }
}
