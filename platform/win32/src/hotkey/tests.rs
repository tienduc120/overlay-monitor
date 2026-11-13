use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_HOTKEY};

use super::{Hotkey, HotkeyError, HotkeyKeys};
use crate::ui::{Flow, MessageWindow, UiEvent, run_message_loop};

/// `VK_F24` with every modifier: a combination nothing else uses.
const COMBINATION: HotkeyKeys = HotkeyKeys {
    ctrl: true,
    shift: true,
    alt: true,
    key: 0x87,
};

/// Each test has its own key, as nextest runs them in parallel processes and a combination is global.
const OTHER: HotkeyKeys = HotkeyKeys {
    key: 0x83,
    ..COMBINATION
};

#[test]
fn a_combination_registered_twice_is_taken() {
    let window = MessageWindow::create().unwrap();
    let first = Hotkey::register(&window, 1, COMBINATION).unwrap();

    let second = Hotkey::register(&window, 2, COMBINATION);

    assert!(matches!(second, Err(HotkeyError::Taken)), "got {second:?}");
    drop(first);
}

#[test]
fn a_dropped_hotkey_frees_its_combination() {
    let window = MessageWindow::create().unwrap();
    drop(Hotkey::register(&window, 1, OTHER).unwrap());

    let again = Hotkey::register(&window, 1, OTHER);

    assert!(again.is_ok(), "got {again:?}");
}

#[test]
fn a_hotkey_message_reaches_the_loop_with_its_id() {
    let window = MessageWindow::create().unwrap();
    // SAFETY: the window is alive on this thread; the message carries plain values.
    unsafe { PostMessageW(Some(window.handle()), WM_HOTKEY, WPARAM(7), LPARAM(0)) }.unwrap();
    let mut seen = Vec::new();

    run_message_loop(&window, &mut |event| {
        seen.push(event);
        Flow::Exit
    });

    assert_eq!(seen, [UiEvent::Hotkey { id: 7 }]);
}
