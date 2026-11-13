use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use thiserror::Error;

/// Which modifier keys a shortcut holds down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShortcutError {
    #[error("a shortcut is its modifiers and exactly one key")]
    NotOneKey,
    #[error("a shortcut needs Ctrl or Alt, or it would take over typing")]
    NoCtrlOrAlt,
    #[error("unknown key `{0}`; use A to Z, 0 to 9, or F1 to F24")]
    UnknownKey(String),
}

/// A global hotkey: modifiers and a Windows virtual-key code (`A` to `Z`, `0` to `9`, `F1` to `F24`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    modifiers: Modifiers,
    key: u8,
}

const F1: u8 = 0x70;
const F24: u8 = 0x87;

impl Shortcut {
    pub const SHOW_HIDE: Self = Self {
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            alt: false,
        },
        key: b'O',
    };
    pub const LOCK_UNLOCK: Self = Self {
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            alt: false,
        },
        key: b'L',
    };

    /// # Errors
    /// `UnknownKey` when `key` is not a supported virtual-key code, `NoCtrlOrAlt` when neither is held.
    pub fn new(modifiers: Modifiers, key: u8) -> Result<Self, ShortcutError> {
        if key_name(key).is_none() {
            return Err(ShortcutError::UnknownKey(shortcut_text(Modifiers::default(), key)));
        }
        if !(modifiers.ctrl || modifiers.alt) {
            return Err(ShortcutError::NoCtrlOrAlt);
        }
        Ok(Self { modifiers, key })
    }

    /// Reads text like `Ctrl+Shift+O`, ignoring case, spaces, and the order of the parts.
    ///
    /// # Errors
    /// `NotOneKey`, `UnknownKey`, or `NoCtrlOrAlt`, checked in that order.
    pub fn parse(text: &str) -> Result<Self, ShortcutError> {
        let mut modifiers = Modifiers::default();
        let mut keys: Vec<&str> = Vec::new();
        for part in text.split('+').map(str::trim) {
            if part.eq_ignore_ascii_case("ctrl") {
                modifiers.ctrl = true;
            } else if part.eq_ignore_ascii_case("alt") {
                modifiers.alt = true;
            } else if part.eq_ignore_ascii_case("shift") {
                modifiers.shift = true;
            } else {
                keys.push(part);
            }
        }
        let [name] = keys.as_slice() else {
            return Err(ShortcutError::NotOneKey);
        };
        if name.is_empty() {
            return Err(ShortcutError::NotOneKey);
        }
        let key = key_code(name).ok_or_else(|| ShortcutError::UnknownKey(String::from(*name)))?;
        Self::new(modifiers, key)
    }

    #[must_use]
    pub const fn modifiers(self) -> Modifiers {
        self.modifiers
    }

    #[must_use]
    pub const fn key(self) -> u8 {
        self.key
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&shortcut_text(self.modifiers, self.key))
    }
}

/// The text for any modifiers and key, in Microsoft's order (Ctrl, Alt, Shift, key). Total: an unknown key prints as
/// hex so a stored value can always be shown.
#[must_use]
pub fn shortcut_text(modifiers: Modifiers, key: u8) -> String {
    let mut text = String::new();
    for (held, name) in [
        (modifiers.ctrl, "Ctrl"),
        (modifiers.alt, "Alt"),
        (modifiers.shift, "Shift"),
    ] {
        if held {
            text.push_str(name);
            text.push('+');
        }
    }
    text.push_str(&key_name(key).unwrap_or_else(|| format!("0x{key:02X}")));
    text
}

fn key_name(key: u8) -> Option<String> {
    match key {
        b'A'..=b'Z' | b'0'..=b'9' => Some(char::from(key).to_string()),
        F1..=F24 => Some(format!("F{}", key - F1 + 1)),
        _ => None,
    }
}

fn key_code(name: &str) -> Option<u8> {
    let name = name.to_ascii_uppercase();
    let bytes = name.as_bytes();
    match bytes {
        [c @ (b'A'..=b'Z' | b'0'..=b'9')] => Some(*c),
        [b'F', digits @ ..] if !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) => {
            let number: u8 = name.get(1..)?.parse().ok()?;
            (1..=24).contains(&number).then(|| F1 + number - 1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
