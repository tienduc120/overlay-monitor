use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

use super::{ForegroundWindow, presenting_process};
use crate::ui::MessageWindow;

#[test]
fn restoring_a_window_that_no_longer_exists_reports_false() {
    let window = MessageWindow::create().unwrap();
    let handle = ForegroundWindow::of(window.handle());
    // SAFETY: the window was created on this thread and is destroyed once; `Drop` ignores the failed second destroy.
    unsafe { DestroyWindow(window.handle()) }.unwrap();

    assert!(!handle.restore());
}

const CORE_WINDOW: &str = "Windows.UI.Core.CoreWindow";
const FRAME_CLASS: &str = "ApplicationFrameWindow";

#[test]
fn a_uwp_frame_maps_to_the_process_of_its_core_window() {
    let children = [(10, CORE_WINDOW.to_owned()), (20, CORE_WINDOW.to_owned())];

    assert_eq!(presenting_process(10, FRAME_CLASS, &children), 20);
}

#[test]
fn an_ordinary_window_keeps_its_own_process() {
    let children = [(20, CORE_WINDOW.to_owned())];

    assert_eq!(presenting_process(10, "UnrealWindow", &children), 10);
}

#[test]
fn a_frame_without_a_foreign_core_window_keeps_the_frame_host() {
    let children = [
        (10, CORE_WINDOW.to_owned()),
        (30, "Windows.UI.Composition.DesktopWindowContentBridge".to_owned()),
    ];

    assert_eq!(presenting_process(10, FRAME_CLASS, &children), 10);
}
