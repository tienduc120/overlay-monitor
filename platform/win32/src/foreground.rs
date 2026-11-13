//! The window the user is working in.

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};
use windows::core::BOOL;

/// The window class of the frame the shell draws around a UWP or Xbox-app game.
const UWP_FRAME_CLASS: &str = "ApplicationFrameWindow";
/// The window class of the UWP app's own window, which belongs to the process that presents.
const UWP_CORE_CLASS: &str = "Windows.UI.Core.CoreWindow";

/// The process that presents the frames of the foreground window, or `None` when no window is in front (the lock
/// screen, a UAC prompt, a window that just closed). A UWP game's frame belongs to `ApplicationFrameHost.exe`, so for
/// it this is the process of the game's own window inside.
pub fn foreground_process_id() -> Option<u32> {
    // SAFETY: the call takes nothing and returns a handle or null.
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return None;
    }
    let owner = window_process(window);
    if owner == 0 {
        return None;
    }
    let class = class_name(window);
    let children = if class == UWP_FRAME_CLASS {
        child_windows(window)
    } else {
        Vec::new()
    };
    Some(presenting_process(owner, &class, &children))
}

/// The process whose frames count for a foreground window owned by `frame_owner` with class `frame_class` and the
/// given child windows (owner, class): a UWP frame hands over to the first core window of another process.
pub fn presenting_process(frame_owner: u32, frame_class: &str, children: &[(u32, String)]) -> u32 {
    if frame_class != UWP_FRAME_CLASS {
        return frame_owner;
    }
    children
        .iter()
        .find(|(owner, class)| *owner != frame_owner && class == UWP_CORE_CLASS)
        .map_or(frame_owner, |(owner, _)| *owner)
}

/// The process that owns `window`, or 0 for one that closed since.
fn window_process(window: HWND) -> u32 {
    let mut process_id = 0;
    // SAFETY: `process_id` is a live local; for a window that closed since, the call fails and leaves it 0.
    unsafe { GetWindowThreadProcessId(window, Some(&raw mut process_id)) };
    process_id
}

fn class_name(window: HWND) -> String {
    let mut buffer = [0_u16; 256];
    // SAFETY: the buffer is a live local the call writes at most its length into.
    let length = unsafe { GetClassNameW(window, &mut buffer) };
    let length = usize::try_from(length).unwrap_or(0);
    String::from_utf16_lossy(buffer.get(..length).unwrap_or_default())
}

fn child_windows(parent: HWND) -> Vec<(u32, String)> {
    let mut children = Vec::new();
    // SAFETY: `collect_child` only runs during this call and treats the parameter as the `Vec` borrowed here.
    // The result says whether the walk was cut short, which a collector never does.
    unsafe { EnumChildWindows(Some(parent), Some(collect_child), LPARAM((&raw mut children) as isize)) }.as_bool();
    children
}

/// Adds one child window to the `Vec` that `child_windows` passed in.
///
/// # Safety
/// Only `EnumChildWindows` calls this, with the `LPARAM` that `child_windows` built from `&mut Vec<(u32, String)>`.
unsafe extern "system" fn collect_child(window: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: per this function's contract the parameter is that `Vec`, borrowed for the whole enumeration.
    if let Some(children) = unsafe { (parameter.0 as *mut Vec<(u32, String)>).as_mut() } {
        children.push((window_process(window), class_name(window)));
    }
    BOOL::from(true)
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
