//! The window the user is working in.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};

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

/// A window that was in front, remembered to be put back in front later.
#[derive(Clone, Copy, Debug)]
pub struct ForegroundWindow(HWND);

impl ForegroundWindow {
    /// The window in front now, or `None` when there is none.
    pub fn current() -> Option<Self> {
        // SAFETY: the call takes nothing and returns a handle or null.
        let window = unsafe { GetForegroundWindow() };
        (!window.is_invalid()).then_some(Self::of(window))
    }

    pub(crate) fn of(window: HWND) -> Self {
        Self(window)
    }

    /// Brings the window back in front; `false` when it is gone or Windows refused.
    pub fn restore(self) -> bool {
        // SAFETY: querying any handle is allowed.
        if !unsafe { IsWindow(Some(self.0)) }.as_bool() {
            return false;
        }
        // SAFETY: the window existed a moment ago; a window gone since only makes the call fail.
        unsafe { SetForegroundWindow(self.0) }.as_bool()
    }
}

#[cfg(test)]
mod tests;
