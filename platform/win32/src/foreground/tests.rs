use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

use super::ForegroundWindow;
use crate::ui::MessageWindow;

#[test]
fn restoring_a_window_that_no_longer_exists_reports_false() {
    let window = MessageWindow::create().unwrap();
    let handle = ForegroundWindow::of(window.handle());
    // SAFETY: the window was created on this thread and is destroyed once; `Drop` ignores the failed second destroy.
    unsafe { DestroyWindow(window.handle()) }.unwrap();

    assert!(!handle.restore());
}
