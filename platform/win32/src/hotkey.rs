//! Global hotkeys, delivered to the message window as [`UiEvent::Hotkey`](crate::ui::UiEvent::Hotkey).

use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, HWND};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};

use crate::Win32Error;
use crate::ui::MessageWindow;

/// A key combination: modifiers and a Windows virtual-key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyKeys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    /// Another program, or another registration of this one, holds the combination.
    #[error("another program already uses this key combination")]
    Taken,
    #[error("register the hotkey")]
    Other(#[source] Win32Error),
}

/// A registered hotkey; dropping it gives the combination back.
#[derive(Debug)]
pub struct Hotkey {
    window: HWND,
    id: i32,
}

impl Hotkey {
    /// Registers `keys` so that pressing them posts `WM_HOTKEY` with `id` to the window, once per press.
    ///
    /// # Errors
    /// `Taken` when the combination is in use, `Other` for any other refusal.
    pub fn register(window: &MessageWindow, id: i32, keys: HotkeyKeys) -> Result<Self, HotkeyError> {
        let mut modifiers = MOD_NOREPEAT;
        for (held, flag) in [(keys.ctrl, MOD_CONTROL), (keys.shift, MOD_SHIFT), (keys.alt, MOD_ALT)] {
            if held {
                modifiers |= flag;
            }
        }
        register(window.handle(), id, modifiers, keys.key)
    }
}

fn register(window: HWND, id: i32, modifiers: HOT_KEY_MODIFIERS, key: u8) -> Result<Hotkey, HotkeyError> {
    // SAFETY: the window is alive and belongs to this thread, which runs its message loop.
    unsafe { RegisterHotKey(Some(window), id, modifiers, u32::from(key)) }.map_err(|error| {
        if error.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() {
            HotkeyError::Taken
        } else {
            HotkeyError::Other(error)
        }
    })?;
    Ok(Hotkey { window, id })
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        // SAFETY: the same window and id that were registered. A window destroyed first took its hotkeys with it, so
        // a failure leaves nothing to undo.
        unsafe { UnregisterHotKey(Some(self.window), self.id) }.ok();
    }
}

#[cfg(test)]
mod tests;
