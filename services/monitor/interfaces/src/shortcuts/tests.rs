use monitor_application::{Settings, ShortcutKeys};
use platform_win32::hotkey::{Hotkey, HotkeyError, HotkeyKeys};
use platform_win32::ui::MessageWindow;

use super::{ShortcutTaken, Shortcuts, hotkey_keys, shortcut_keys};
use crate::settings_editor::ShortcutSlot;

#[test]
fn keys_convert_both_ways_unchanged() {
    let keys = ShortcutKeys {
        ctrl: true,
        shift: false,
        alt: true,
        key: b'L',
    };

    let there = hotkey_keys(keys);
    assert_eq!(
        there,
        HotkeyKeys {
            ctrl: true,
            shift: false,
            alt: true,
            key: b'L'
        }
    );
    assert_eq!(shortcut_keys(there), keys);
}

fn ctrl_shift(key: u8) -> ShortcutKeys {
    ShortcutKeys {
        ctrl: true,
        shift: true,
        alt: false,
        key,
    }
}

fn alt_shift(key: u8) -> ShortcutKeys {
    ShortcutKeys {
        ctrl: false,
        shift: true,
        alt: true,
        key,
    }
}

fn ctrl_only(key: u8) -> ShortcutKeys {
    ShortcutKeys {
        ctrl: true,
        shift: false,
        alt: false,
        key,
    }
}

fn is_taken(window: &MessageWindow, keys: ShortcutKeys) -> bool {
    matches!(Hotkey::register(window, 50, hotkey_keys(keys)), Err(HotkeyError::Taken))
}

fn settings_with(show_hide: ShortcutKeys, lock_unlock: ShortcutKeys) -> Settings {
    Settings {
        show_hide,
        lock_unlock,
        ..Settings::default()
    }
}

// F13 to F15 under a different set of modifiers in each test: tests run in parallel processes, and no other test uses
// these combinations.
const F13: u8 = 0x7C;
const F14: u8 = 0x7D;
const F15: u8 = 0x7E;

#[test]
fn suspending_gives_both_combinations_back() {
    let window = MessageWindow::create().unwrap();
    let other = MessageWindow::create().unwrap();
    let mut shortcuts = Shortcuts::register(&window, &settings_with(ctrl_shift(F13), ctrl_shift(F14)));
    assert!(is_taken(&other, ctrl_shift(F13)) && is_taken(&other, ctrl_shift(F14)));

    shortcuts.suspend();

    assert!(!is_taken(&other, ctrl_shift(F13)) && !is_taken(&other, ctrl_shift(F14)));
    assert!(!shortcuts.lock_unlock_registered());
}

#[test]
fn replacing_registers_the_new_pair_even_when_one_stays_the_same() {
    let window = MessageWindow::create().unwrap();
    let other = MessageWindow::create().unwrap();
    let mut shortcuts = Shortcuts::register(&window, &settings_with(alt_shift(F13), alt_shift(F14)));

    let outcome = shortcuts.replace(&window, &settings_with(alt_shift(F15), alt_shift(F14)));

    assert_eq!(outcome, Ok(()));
    assert!(is_taken(&other, alt_shift(F15)) && is_taken(&other, alt_shift(F14)));
    assert!(!is_taken(&other, alt_shift(F13)), "the old one is given back");
    assert!(shortcuts.lock_unlock_registered());
}

#[test]
fn replacing_names_the_taken_shortcut_and_leaves_nothing_registered() {
    let window = MessageWindow::create().unwrap();
    let other = MessageWindow::create().unwrap();
    let held = Hotkey::register(&other, 9, hotkey_keys(ctrl_only(F15))).unwrap();
    let mut shortcuts = Shortcuts::register(&window, &settings_with(ctrl_only(F13), ctrl_only(F14)));

    let outcome = shortcuts.replace(&window, &settings_with(ctrl_only(F13), ctrl_only(F15)));

    assert_eq!(
        outcome,
        Err(ShortcutTaken {
            slot: ShortcutSlot::LockUnlock,
            keys: ctrl_only(F15)
        })
    );
    assert!(
        !is_taken(&other, ctrl_only(F13)),
        "what it registered before the failure is dropped"
    );
    drop(held);
}
