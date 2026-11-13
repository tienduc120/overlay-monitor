//! The notification-area icon: left-click and right-click reach the message window as [`crate::ui::UiEvent`]s.

use std::mem::size_of;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION, NOTIFYICON_VERSION_4,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::HICON;

use crate::Win32Error;
use crate::icon::Icon;
use crate::ui::{MessageWindow, TRAY_CALLBACK};

/// The one icon this process has; the shell tells icons of a window apart by this number.
const ICON_ID: u32 = 1;

/// The icon in the notification area; dropping it removes the icon. Drop it before the [`MessageWindow`] it reports
/// to, as the shell finds the icon by that window.
pub struct TrayIcon {
    window: HWND,
    tooltip: String,
    /// Borrowed from the [`Icon`] the caller keeps, which must outlive this.
    icon: HICON,
}

impl TrayIcon {
    /// Makes the icon without showing it: the shell may not be ready, so `readd` adds it, now or when the taskbar comes.
    /// `icon` must outlive the tray icon, as `readd` needs the handle.
    #[must_use]
    pub fn create(window: &MessageWindow, tooltip: &str, icon: &Icon) -> Self {
        Self {
            window: window.handle(),
            tooltip: tooltip.to_owned(),
            icon: icon.handle(),
        }
    }

    /// Adds the icon, the first time or after Explorer restarted and dropped it.
    ///
    /// # Errors
    /// The shell refused the icon.
    pub fn readd(&self) -> Result<(), Win32Error> {
        // A copy still there would make the add fail; one that is gone makes the delete fail, which does not matter.
        Self::notify(NIM_DELETE, &self.data()).ok();
        self.register()
    }

    fn register(&self) -> Result<(), Win32Error> {
        let mut data = self.data();
        Self::notify(NIM_ADD, &data)?;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Self::notify(NIM_SETVERSION, &data)
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(u32::MAX),
            hWnd: self.window,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_CALLBACK,
            ..NOTIFYICONDATAW::default()
        };
        data.hIcon = self.icon;
        // The tip holds 127 units and a NUL; a longer text is cut, and the array is zeroed, so it stays terminated.
        for (slot, unit) in data.szTip.iter_mut().take(127).zip(self.tooltip.encode_utf16()) {
            *slot = unit;
        }
        data
    }

    fn notify(
        message: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE,
        data: &NOTIFYICONDATAW,
    ) -> Result<(), Win32Error> {
        // SAFETY: `data` is a live, fully initialised structure with its size set.
        if unsafe { Shell_NotifyIconW(message, data) }.as_bool() {
            Ok(())
        } else {
            Err(Win32Error::from_thread())
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        // A failed removal leaves the shell to drop the icon when the process ends.
        Self::notify(NIM_DELETE, &self.data()).ok();
    }
}

#[cfg(test)]
mod tests;
