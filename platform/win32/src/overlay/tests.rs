use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use windows::Win32::Foundation::{DWRITE_E_NOFONT, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, HTCAPTION, HWND_NOTOPMOST, IsWindow, MA_NOACTIVATE, MSG, PM_REMOVE,
    PeekMessageW, PostMessageW, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW, SetWindowPos, SystemParametersInfoW, WM_CLOSE, WM_EXITSIZEMOVE,
    WM_MOUSEACTIVATE, WM_NCHITTEST, WM_NCRBUTTONUP, WM_QUIT, WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

use super::{OverlayWindow, on_moving, premultiplied_bgra, screen_point, shared, snap_distance, window_size};
use crate::placement::MonitorPlacement;
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

#[test]
fn closing_the_overlay_asks_the_loop_to_end_instead_of_destroying_it() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();

    send(&window, WM_CLOSE, WPARAM(0), LPARAM(0));

    // SAFETY: querying any handle is allowed.
    let alive = unsafe { IsWindow(Some(window.window)) }.as_bool();
    assert!(alive, "the window must outlive a close request");
    let mut message = MSG::default();
    // SAFETY: `message` is a live local; only WM_QUIT is asked for, and it is removed so nothing else sees it.
    let quit = unsafe { PeekMessageW(&raw mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE) };
    assert!(quit.as_bool(), "a close request must post WM_QUIT");
}

#[test]
fn the_snap_distance_grows_with_the_dpi() {
    assert_eq!(snap_distance(96), 12);
    assert_eq!(snap_distance(120), 15, "125 % scaling");
    assert_eq!(snap_distance(192), 24);
}

#[test]
fn a_drag_across_the_left_edge_of_the_primary_monitor_is_written_back_flush() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();
    let mut proposed = RECT {
        left: -30,
        top: 400,
        right: 170,
        bottom: 500,
    };

    // SAFETY: called as the window procedure would, on the window's own thread.
    let handled = on_moving(
        window.window,
        unsafe { shared(window.window) },
        &mut proposed,
        POINT { x: 0, y: 0 },
    );

    assert!(handled, "the rectangle must be rewritten");
    assert_eq!(
        (proposed.left, proposed.right, proposed.top, proposed.bottom),
        (0, 200, 400, 500),
        "flush with the monitor's left edge, same size"
    );
}

#[test]
fn a_snapped_window_that_grows_stays_flush_with_the_edge() {
    let mut window = OverlayWindow::create(EventSink::discard()).unwrap();
    window.draw(&lines(&["FPS:   30 FPS"]), &style()).unwrap();
    let mut proposed = window_rect(&window);
    proposed.left -= 30;
    proposed.right -= 30;
    // SAFETY: as above.
    on_moving(
        window.window,
        unsafe { shared(window.window) },
        &mut proposed,
        POINT { x: 0, y: 0 },
    );
    // SAFETY: the window is alive, and `proposed` is the flush rectangle.
    unsafe {
        SetWindowPos(
            window.window,
            None,
            proposed.left,
            proposed.top,
            proposed.right - proposed.left,
            proposed.bottom - proposed.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
    .unwrap();

    window
        .draw(&lines(&["FPS:   30 FPS", "CPU:   2.0 %, 45.8 °C"]), &style())
        .unwrap();

    assert_eq!(window_rect(&window).left, 0, "growing must not pull it off the edge");
}

#[test]
fn the_placement_names_the_monitor_and_the_offset_within_it() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();
    window.move_to(None);

    let placement = window.placement().unwrap();

    assert!(!placement.monitor.is_empty(), "the monitor must be named");
    let work = primary_work_area();
    let monitor = primary_monitor_bounds();
    assert_eq!(
        (placement.x, placement.y),
        (work.left - monitor.left, work.top - monitor.top),
        "with nothing saved the overlay sits at the primary work area's origin"
    );
}

#[test]
fn a_saved_placement_moves_the_window_back_to_it() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();
    window.move_to(None);
    let home = window.placement().unwrap();
    let elsewhere = MonitorPlacement {
        x: home.x + 40,
        y: home.y + 30,
        monitor: home.monitor,
    };

    window.move_to(Some(&elsewhere));

    assert_eq!(window.placement(), Some(elsewhere));
}

#[test]
fn a_window_restored_flush_right_stays_flush_when_it_narrows() {
    let mut window = OverlayWindow::create(EventSink::discard()).unwrap();
    let wide = lines(&["FPS:   30 FPS", "CPU:   2.0 %, 45.8 °C"]);
    window.draw(&wide, &style()).unwrap();
    window.move_to(None);
    let home = window.placement().unwrap();
    let monitor = primary_monitor_bounds();
    let width = size(window_rect(&window)).0;
    let flush_right = MonitorPlacement {
        x: monitor.right - monitor.left - width,
        y: home.y,
        monitor: home.monitor,
    };
    window.move_to(Some(&flush_right));
    window.draw(&wide, &style()).unwrap();

    window.draw(&lines(&["FPS:   30 FPS"]), &style()).unwrap();

    let narrowed = window_rect(&window);
    assert!(size(narrowed).0 < width, "the test needs a narrower window");
    assert_eq!(narrowed.right, monitor.right, "a gap must not open on the right");
}

#[test]
fn a_finished_drag_is_reported() {
    let (sink, events) = recording_sink();
    let window = OverlayWindow::create(sink).unwrap();

    send(&window, WM_EXITSIZEMOVE, WPARAM(0), LPARAM(0));

    assert_eq!(*events.borrow(), [UiEvent::OverlayMoved]);
}

fn primary_monitor_bounds() -> RECT {
    // SAFETY: plain values; the screen origin is always on the primary monitor.
    let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap(),
        ..MONITORINFO::default()
    };
    // SAFETY: `info` is a live local with its size set.
    unsafe { GetMonitorInfoW(monitor, &raw mut info) }.ok().unwrap();
    info.rcMonitor
}

fn ex_style(window: &OverlayWindow) -> usize {
    // SAFETY: reading the window's extended style has no preconditions.
    unsafe { GetWindowLongPtrW(window.window, GWL_EXSTYLE) }.cast_unsigned()
}

#[test]
fn click_through_comes_and_goes_with_the_lock() {
    let window = OverlayWindow::create(EventSink::discard()).unwrap();
    let transparent = WS_EX_TRANSPARENT.0 as usize;
    let layered = WS_EX_LAYERED.0 as usize;

    window.set_click_through(true);
    let locked = ex_style(&window);
    window.set_click_through(false);
    let unlocked = ex_style(&window);

    assert_ne!(locked & transparent, 0, "a locked overlay lets clicks through");
    assert_eq!(unlocked & transparent, 0, "an unlocked one can be grabbed");
    assert_ne!(locked & layered, 0, "it stays layered");
    assert_ne!(unlocked & layered, 0, "it stays layered");
}

/// What the overlay draws, as its window shows it: premultiplied BGRA rows after a little-endian width and height, in
/// `%TEMP%\overlay-monitor-pixels\<name>.bgra`, so a renderer change can be compared pixel by pixel.
#[test]
#[ignore = "writes the overlay's pixels to the temp folder for a visual check"]
fn writes_the_drawn_pixels_for_a_visual_check() {
    crate::dpi::enable_per_monitor_awareness().unwrap();
    let folder = std::env::temp_dir().join("overlay-monitor-pixels");
    std::fs::create_dir_all(&folder).unwrap();
    let four = lines(&[
        "FPS:   30 FPS",
        "CPU:   2.0 %, 45.8 °C",
        "GPU:   4.0 %, 56.0 °C",
        "DDR5: 30.9 %, 35.5 °C",
    ]);
    let varied = TextStyle {
        font_family: String::from("Segoe UI"),
        size_pt: 20.0,
        bold: false,
        italic: true,
        text_rgb: 0xFF_E0_40,
        outline_rgb: 0x40_00_80,
        outline_px: 3.0,
        background_rgb: 0x00_40_FF,
        background_alpha: 128,
    };
    let styles = [
        ("default", style()),
        (
            "1px",
            TextStyle {
                outline_px: 1.0,
                ..style()
            },
        ),
        (
            "4px",
            TextStyle {
                outline_px: 4.0,
                ..style()
            },
        ),
        ("varied", varied),
    ];
    for (name, style) in styles {
        let mut window = OverlayWindow::create(EventSink::discard()).unwrap();
        window.draw(&four, &style).unwrap();
        let mut canvas = window.shared.canvas.borrow_mut();
        let surface = canvas.surface.as_mut().unwrap();
        let mut bytes = Vec::new();
        bytes.extend(surface.size.cx.to_le_bytes());
        bytes.extend(surface.size.cy.to_le_bytes());
        bytes.extend(surface.pixels_mut().as_flattened());
        std::fs::write(folder.join(format!("{name}.bgra")), bytes).unwrap();
    }
}
