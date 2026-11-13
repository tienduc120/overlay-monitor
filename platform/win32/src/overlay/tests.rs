use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use windows::Win32::Foundation::{DWRITE_E_NOFONT, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, HTCAPTION, HWND_NOTOPMOST, MA_NOACTIVATE, PostMessageW,
    SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW,
    SetWindowPos, SystemParametersInfoW, WM_MOUSEACTIVATE, WM_NCHITTEST, WM_NCRBUTTONUP, WS_EX_TOPMOST,
};

use super::{OverlayWindow, premultiplied_bgra, screen_point, window_size};
use crate::text::TextStyle;
use crate::ui::{EventSink, UiEvent, pump_messages_for};

#[test]
fn the_window_is_the_text_plus_outline_and_padding_on_every_side() {
    assert_eq!(
        window_size(200.0, 80.0, 2.0),
        (216, 96),
        "200 + 2*(2*2+4) by 80 + 2*(2*2+4)"
    );
}

#[test]
fn a_fractional_text_size_rounds_the_window_up() {
    assert_eq!(
        window_size(200.2, 80.5, 0.0),
        (209, 89),
        "the last partial pixel of text must still fit"
    );
}

#[test]
fn colours_are_premultiplied_by_their_alpha() {
    assert_eq!(premultiplied_bgra(0x00_FF_00, 128), [0, 128, 0, 128]);
    assert_eq!(premultiplied_bgra(0xFF_FF_FF, 0), [0, 0, 0, 0]);
}

#[test]
fn the_channels_are_stored_blue_green_red_alpha() {
    assert_eq!(premultiplied_bgra(0x11_22_33, 255), [0x33, 0x22, 0x11, 255]);
}

#[test]
fn a_click_left_of_or_above_the_primary_monitor_keeps_its_negative_coordinate() {
    assert_eq!(
        screen_point(LPARAM(0x012C_FFFB)),
        (-5, 300),
        "x is the low word, y the high word, both signed"
    );
}

#[test]
fn the_window_starts_hidden_at_the_top_left_of_the_primary_work_area() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();

    let rect = window_rect(&window);

    assert!(!window.is_visible(), "the overlay must not show before it is asked to");
    let work = primary_work_area();
    assert_eq!((rect.left, rect.top), (work.left, work.top));
}

#[test]
fn the_whole_window_drags_it_and_a_click_never_takes_focus() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();

    let hit = send(&window, WM_NCHITTEST, WPARAM(0), LPARAM(0));
    let activate = send(&window, WM_MOUSEACTIVATE, WPARAM(0), LPARAM(0));

    assert_eq!(
        hit.0,
        isize::try_from(HTCAPTION).unwrap(),
        "every point must report itself as the caption"
    );
    assert_eq!(
        activate.0,
        isize::try_from(MA_NOACTIVATE).unwrap(),
        "a click must leave the game in front"
    );
}

#[test]
fn a_right_click_asks_for_the_overlay_menu_at_the_pointer() {
    let (sink, events) = recording_sink();
    let window = OverlayWindow::create(sink).unwrap();

    send(&window, WM_NCRBUTTONUP, caption(), LPARAM(0x012C_FFFB));

    assert_eq!(*events.borrow(), [UiEvent::OverlayMenu { x: -5, y: 300 }]);
}

#[test]
fn a_posted_right_click_reaches_the_sink_once_messages_are_pumped() {
    let (sink, events) = recording_sink();
    let window = OverlayWindow::create(sink).unwrap();
    // SAFETY: the window is alive on this thread, and the message carries plain values.
    unsafe { PostMessageW(Some(window.window), WM_NCRBUTTONUP, caption(), LPARAM(0x0002_0001)) }.unwrap();

    pump_messages_for(Duration::from_millis(50));

    assert_eq!(*events.borrow(), [UiEvent::OverlayMenu { x: 1, y: 2 }]);
}

#[test]
fn drawing_sizes_the_window_to_its_text_and_leaves_it_hidden() {
    let mut window = OverlayWindow::create(EventSink::discard()).unwrap();

    window.draw(&lines(&["FPS:   30 FPS"]), &style()).unwrap();
    let one = size(window_rect(&window));
    window
        .draw(&lines(&["FPS:   30 FPS", "CPU:   2.0 %, 45.8 °C"]), &style())
        .unwrap();
    let two = size(window_rect(&window));

    assert!(one.0 > 0 && one.1 > 0, "a drawn line must have a size: {one:?}");
    assert!(
        two.0 > one.0,
        "a longer line must widen the window: {one:?} then {two:?}"
    );
    assert!(two.1 > one.1, "another line must make it taller: {one:?} then {two:?}");
    assert!(!window.is_visible(), "drawing must not show a hidden overlay");
}

#[test]
fn a_font_that_is_not_installed_is_an_error_not_a_blank_overlay() {
    let mut window = OverlayWindow::create(EventSink::discard()).unwrap();
    let style = TextStyle {
        font_family: String::from("No Such Font Family 4c1e"),
        ..style()
    };

    let drawn = window.draw(&lines(&["FPS:   30 FPS"]), &style);

    assert_eq!(
        drawn.unwrap_err().code(),
        DWRITE_E_NOFONT,
        "drawing in a missing font must fail"
    );
}

#[test]
fn show_and_hide_change_whether_it_is_visible() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();

    window.show();
    let shown = window.is_visible();
    window.hide();
    let hidden = !window.is_visible();

    assert!(shown && hidden, "shown: {shown}, hidden afterwards: {hidden}");
}

fn style() -> TextStyle {
    TextStyle {
        font_family: String::from("Consolas"),
        size_pt: 14.0,
        bold: true,
        italic: false,
        text_rgb: 0x00_FF_00,
        outline_rgb: 0x00_00_00,
        outline_px: 2.0,
        background_rgb: 0x00_00_00,
        background_alpha: 1,
    }
}

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|&line| String::from(line)).collect()
}

fn recording_sink() -> (EventSink, Rc<RefCell<Vec<UiEvent>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&events);
    (EventSink::new(move |event| recorded.borrow_mut().push(event)), events)
}

fn caption() -> WPARAM {
    WPARAM(usize::try_from(HTCAPTION).unwrap())
}

fn send(window: &OverlayWindow, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: the window is alive on this thread, and the message carries plain values.
    unsafe { SendMessageW(window.window, message, Some(wparam), Some(lparam)) }
}

fn window_rect(window: &OverlayWindow) -> RECT {
    let mut rect = RECT::default();
    // SAFETY: the window is alive, and `rect` is a live local.
    unsafe { GetWindowRect(window.window, &raw mut rect) }.unwrap();
    rect
}

fn size(rect: RECT) -> (i32, i32) {
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn primary_work_area() -> RECT {
    let mut work = RECT::default();
    // SAFETY: `work` is a live local of the type SPI_GETWORKAREA writes.
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut work).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .unwrap();
    work
}

#[test]
fn keeping_on_top_restores_a_lost_topmost_position_without_moving_or_showing_it() {
    let mut window = OverlayWindow::create(EventSink::discard()).unwrap();
    window.draw(&lines(&["FPS:   30 FPS"]), &style()).unwrap();
    let before = window_rect(&window);
    // SAFETY: the window is alive; the flags keep it where it is, as it is, and unactivated.
    unsafe {
        SetWindowPos(
            window.window,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    }
    .unwrap();
    assert!(!is_topmost(&window), "the test must start with the window demoted");

    window.keep_on_top();

    assert!(
        is_topmost(&window),
        "the overlay must be back above other topmost windows"
    );
    assert_eq!(window_rect(&window), before, "it must not move or resize");
    assert!(!window.is_visible(), "it must not show a hidden overlay");
}

fn is_topmost(window: &OverlayWindow) -> bool {
    // SAFETY: reading the window's extended style has no preconditions.
    let style = unsafe { GetWindowLongPtrW(window.window, GWL_EXSTYLE) };
    style.cast_unsigned() & WS_EX_TOPMOST.0 as usize != 0
}
