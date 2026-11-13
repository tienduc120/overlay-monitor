//! The popup menu both the tray icon and the overlay open.

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MF_CHECKED, MF_STRING, PostMessageW, SetForegroundWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
};
use windows::core::PCWSTR;

use crate::ui::MessageWindow;

/// One entry of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// What [`show_popup_menu`] answers when the entry is chosen; never 0, which means no choice.
    pub id: u16,
    pub text: String,
    pub checked: bool,
}

/// Shows the menu at the screen point and waits for a choice: `None` when it was dismissed or could not be shown.
pub fn show_popup_menu(owner: &MessageWindow, items: &[MenuItem], x: i32, y: i32) -> Option<u16> {
    // SAFETY: no preconditions; the menu is destroyed below.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    let chosen = track(menu, owner, items, x, y);
    // SAFETY: the menu was created above and is destroyed once.
    unsafe { DestroyMenu(menu) }.ok();
    chosen
}

fn track(menu: HMENU, owner: &MessageWindow, items: &[MenuItem], x: i32, y: i32) -> Option<u16> {
    for item in items {
        let text: Vec<u16> = item.text.encode_utf16().chain(Some(0)).collect();
        let flags = if item.checked {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        // SAFETY: `text` is NUL-terminated and outlives the call, which copies it.
        unsafe { AppendMenuW(menu, flags, usize::from(item.id), PCWSTR(text.as_ptr())) }.ok()?;
    }
    let window = owner.handle();
    // Without the foreground, the menu would not close when the user clicks elsewhere. A refusal only costs that.
    // SAFETY: the window is alive.
    unsafe { SetForegroundWindow(window) }.as_bool();
    // SAFETY: the menu and the window are alive on this thread; TPM_RETURNCMD makes the call answer the choice
    // instead of posting it, and no exclusion rectangle is passed.
    let command = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, None, window, None) };
    // The documented fix for a menu that would not close again when its next click is on the same window.
    // SAFETY: the window is alive; the message carries no data.
    unsafe { PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0)) }.ok();
    u16::try_from(command.0).ok().filter(|&id| id != 0)
}

#[cfg(test)]
mod tests;
