//! The window the user is working in.

use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// The process that owns the foreground window, or `None` when no window is in front (the lock screen, a UAC
/// prompt, a window that just closed).
pub fn foreground_process_id() -> Option<u32> {
    // SAFETY: the call takes nothing and returns a handle or null.
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return None;
    }
    let mut process_id = 0;
    // SAFETY: `process_id` is a live local; for a window that closed since, the call fails and leaves it 0.
    unsafe { GetWindowThreadProcessId(window, Some(&raw mut process_id)) };
    (process_id != 0).then_some(process_id)
}
