use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CANCELMODE};

use super::{MenuItem, show_popup_menu};
use crate::ui::MessageWindow;

#[test]
fn a_menu_dismissed_without_a_choice_answers_none() {
    let window = MessageWindow::create().unwrap();
    let items = [MenuItem {
        id: 1,
        text: String::from("Show overlay"),
        checked: true,
    }];
    // SAFETY: the window is alive on this thread; the posted message carries no data. It reaches the menu's own
    // loop and dismisses it.
    unsafe { PostMessageW(Some(window.handle()), WM_CANCELMODE, WPARAM(0), LPARAM(0)) }.unwrap();

    let chosen = show_popup_menu(&window, &items, 0, 0);

    assert_eq!(chosen, None);
}
