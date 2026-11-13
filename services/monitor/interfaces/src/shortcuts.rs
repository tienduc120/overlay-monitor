//! The two global shortcuts the settings name, registered on the UI thread's message window.

use monitor_application::{Settings, ShortcutKeys};
use platform_win32::hotkey::{Hotkey, HotkeyError, HotkeyKeys};
use platform_win32::ui::MessageWindow;

use crate::settings_editor::ShortcutSlot;

/// What [`UiEvent::Hotkey`](platform_win32::ui::UiEvent::Hotkey) carries for the show/hide shortcut.
pub(crate) const SHOW_HIDE_HOTKEY: i32 = 1;
/// What it carries for the lock/unlock shortcut.
pub(crate) const LOCK_HOTKEY: i32 = 2;

/// The shortcuts that could be registered; dropping them gives the key combinations back.
#[derive(Debug)]
pub struct Shortcuts {
    show_hide: Option<Hotkey>,
    lock_unlock: Option<Hotkey>,
}

impl Shortcuts {
    /// Registers both shortcuts. One that is taken or fails is logged and left out, so the app still runs.
    pub fn register(window: &MessageWindow, settings: &Settings) -> Self {
        Self {
            show_hide: register_one(window, SHOW_HIDE_HOTKEY, "show or hide", settings.show_hide),
            lock_unlock: register_one(window, LOCK_HOTKEY, "lock or unlock", settings.lock_unlock),
        }
    }

    /// Whether the lock shortcut works, which decides whether a saved lock may stand: a locked overlay can only be
    /// unlocked from the tray menu or by this shortcut.
    pub fn lock_unlock_registered(&self) -> bool {
        self.lock_unlock.is_some()
    }

    /// Gives both combinations back, so a shortcut field can capture them while the settings window is open.
    pub fn suspend(&mut self) {
        self.show_hide = None;
        self.lock_unlock = None;
    }

    /// Registers the shortcuts `new` names in place of the ones held. When one is taken, nothing stays registered and
    /// the caller puts the old ones back with [`Shortcuts::register`].
    ///
    /// # Errors
    /// `ShortcutTaken` for the first shortcut that could not be registered.
    pub(crate) fn replace(&mut self, window: &MessageWindow, new: &Settings) -> Result<(), ShortcutTaken> {
        // The old ones go first: a combination that stays the same would otherwise find itself taken.
        self.suspend();
        let wanted = [
            (ShortcutSlot::ShowHide, SHOW_HIDE_HOTKEY, new.show_hide),
            (ShortcutSlot::LockUnlock, LOCK_HOTKEY, new.lock_unlock),
        ];
        let mut registered = Vec::new();
        for (slot, id, keys) in wanted {
            match Hotkey::register(window, id, hotkey_keys(keys)) {
                Ok(hotkey) => registered.push(hotkey),
                Err(error) => {
                    tracing::info!(?error, %keys, "the shortcut could not be registered");
                    return Err(ShortcutTaken { slot, keys });
                }
            }
        }
        let mut registered = registered.into_iter();
        self.show_hide = registered.next();
        self.lock_unlock = registered.next();
        Ok(())
    }
}

/// A shortcut [`Shortcuts::replace`] could not register: another program holds it, or Windows refused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShortcutTaken {
    pub slot: ShortcutSlot,
    pub keys: ShortcutKeys,
}

fn register_one(window: &MessageWindow, id: i32, what: &str, keys: ShortcutKeys) -> Option<Hotkey> {
    match Hotkey::register(window, id, hotkey_keys(keys)) {
        Ok(hotkey) => Some(hotkey),
        Err(HotkeyError::Taken) => {
            tracing::warn!(%keys, "the {what} shortcut is taken by another program");
            None
        }
        Err(error @ HotkeyError::Other(_)) => {
            tracing::warn!(?error, %keys, "the {what} shortcut could not be registered");
            None
        }
    }
}

pub fn hotkey_keys(keys: ShortcutKeys) -> HotkeyKeys {
    HotkeyKeys {
        ctrl: keys.ctrl,
        shift: keys.shift,
        alt: keys.alt,
        key: keys.key,
    }
}

pub fn shortcut_keys(keys: HotkeyKeys) -> ShortcutKeys {
    ShortcutKeys {
        ctrl: keys.ctrl,
        shift: keys.shift,
        alt: keys.alt,
        key: keys.key,
    }
}

#[cfg(test)]
mod tests;
