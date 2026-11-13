use monitor_application::ShortcutKeys;
use platform_win32::hotkey::HotkeyKeys;

use super::{hotkey_keys, shortcut_keys};

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
