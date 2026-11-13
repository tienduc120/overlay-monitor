use alloc::string::ToString;

use super::{Modifiers, Shortcut, ShortcutError, shortcut_text};

const CTRL: Modifiers = Modifiers {
    ctrl: true,
    shift: false,
    alt: false,
};

#[test]
fn the_default_shortcuts_read_ctrl_shift_o_and_ctrl_shift_l() {
    assert_eq!(Shortcut::SHOW_HIDE.to_string(), "Ctrl+Shift+O");
    assert_eq!(Shortcut::LOCK_UNLOCK.to_string(), "Ctrl+Shift+L");
}

#[test]
fn parsing_ignores_case_spaces_and_order() {
    assert_eq!(
        Shortcut::parse(" shift + ctrl + o ").unwrap().to_string(),
        "Ctrl+Shift+O"
    );
    assert_eq!(
        Shortcut::parse("alt+ctrl+shift+f12").unwrap().to_string(),
        "Ctrl+Alt+Shift+F12"
    );
    assert_eq!(Shortcut::parse("Ctrl+7").unwrap().key(), b'7');
}

#[test]
fn a_shortcut_without_ctrl_or_alt_is_refused() {
    assert_eq!(Shortcut::parse("Shift+O"), Err(ShortcutError::NoCtrlOrAlt));
    assert_eq!(Shortcut::parse("O"), Err(ShortcutError::NoCtrlOrAlt));
}

#[test]
fn anything_but_exactly_one_known_key_is_refused() {
    assert_eq!(Shortcut::parse("Ctrl+"), Err(ShortcutError::NotOneKey));
    assert_eq!(Shortcut::parse("Ctrl+A+B"), Err(ShortcutError::NotOneKey));
    assert_eq!(
        Shortcut::parse("Ctrl+Space"),
        Err(ShortcutError::UnknownKey("Space".into()))
    );
    assert!(matches!(Shortcut::parse("Ctrl+F25"), Err(ShortcutError::UnknownKey(_))));
    assert!(matches!(Shortcut::new(CTRL, 0x20), Err(ShortcutError::UnknownKey(_))));
}

#[test]
fn an_unknown_key_still_prints() {
    assert_eq!(shortcut_text(CTRL, 0x20), "Ctrl+0x20");
}
