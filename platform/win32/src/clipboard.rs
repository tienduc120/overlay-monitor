//! Plain text on the clipboard.

use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
#[cfg(test)]
use windows::Win32::System::DataExchange::GetClipboardData;
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use crate::Win32Error;
use crate::ui::{MessageWindow, pump_messages_for};

/// Another program may hold the clipboard for a moment, so opening it is tried a few times.
const OPEN_TRIES: u32 = 5;
const OPEN_WAIT: Duration = Duration::from_millis(10);

/// Closes the clipboard when dropped.
struct Open;

impl Open {
    fn by(owner: &MessageWindow) -> Result<Self, Win32Error> {
        let mut tries = 1;
        loop {
            // SAFETY: the window handle is valid while `owner` lives.
            match unsafe { OpenClipboard(Some(owner.handle())) } {
                Ok(()) => return Ok(Self),
                Err(_busy) if tries < OPEN_TRIES => {
                    tries += 1;
                    pump_messages_for(OPEN_WAIT);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: this guard opened the clipboard on this thread and closes it once.
        unsafe { CloseClipboard() }.ok();
    }
}

/// Replaces the clipboard's contents with `text`, which `owner` holds until something else is copied.
///
/// # Errors
/// The clipboard could not be opened (another program kept it) or the text could not be stored.
pub fn set_text(owner: &MessageWindow, text: &str) -> Result<(), Win32Error> {
    let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let _open = Open::by(owner)?;
    // SAFETY: the clipboard is open on this thread.
    unsafe { EmptyClipboard() }?;
    let memory = global_copy(&units)?;
    // SAFETY: `memory` is a movable block holding NUL-terminated UTF-16, the format `CF_UNICODETEXT` names.
    let stored = unsafe { SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))) };
    if let Err(error) = stored {
        // SAFETY: the clipboard did not take the block, so it is still ours to free, once.
        unsafe { GlobalFree(Some(memory)) }.ok();
        return Err(error);
    }
    Ok(())
}

/// A movable block of global memory holding `units`.
fn global_copy(units: &[u16]) -> Result<HGLOBAL, Win32Error> {
    // SAFETY: a plain allocation of the size asked for.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) }?;
    // SAFETY: `memory` was just allocated and is not locked elsewhere.
    let target = unsafe { GlobalLock(memory) }.cast::<u16>();
    if target.is_null() {
        let error = Win32Error::from_thread();
        // SAFETY: the block is ours and was never handed on.
        unsafe { GlobalFree(Some(memory)) }.ok();
        return Err(error);
    }
    // SAFETY: the block holds `units.len() * 2` bytes and the source is a separate allocation; the unlock reports an
    // error only when the lock count reaches zero, which is the expected end here.
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr(), target, units.len());
        GlobalUnlock(memory).ok();
    }
    Ok(memory)
}

/// The clipboard's text, for the test that proves `set_text`.
#[cfg(test)]
pub(crate) fn text() -> Option<String> {
    let window = MessageWindow::create().ok()?;
    let _open = Open::by(&window).ok()?;
    // SAFETY: the clipboard is open on this thread.
    let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.ok()?;
    let memory = HGLOBAL(handle.0);
    // SAFETY: the clipboard owns the block; it stays valid while locked and while the clipboard is open.
    let pointer = unsafe { GlobalLock(memory) }.cast::<u16>();
    if pointer.is_null() {
        return None;
    }
    // SAFETY: text in this format is NUL-terminated, so the scan ends inside the block.
    let units = unsafe {
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        std::slice::from_raw_parts(pointer, length)
    };
    let text = String::from_utf16_lossy(units);
    // SAFETY: balances the lock above.
    unsafe { GlobalUnlock(memory) }.ok();
    Some(text)
}

#[cfg(test)]
mod tests;
