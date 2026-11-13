use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::{
    IsWindow, PostMessageW, PostQuitMessage, SendMessageW, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_ENDSESSION,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUERYENDSESSION,
};

use super::{
    Flow, MessageWindow, UiEvent, Waker, allow_message, is_taskbar_created, pump_messages_for, run_message_loop,
    taskbar_created_message, tray_event,
};

#[test]
fn pumping_returns_once_its_time_is_up() {
    let start = Instant::now();

    pump_messages_for(Duration::from_millis(100));

    let took = start.elapsed();
    assert!(took >= Duration::from_millis(100), "returned early, after {took:?}");
    assert!(took < Duration::from_secs(2), "overran: {took:?}");
}

fn loop_events(window: &MessageWindow, exit_on: impl Fn(UiEvent) -> bool) -> Vec<UiEvent> {
    let mut seen = Vec::new();
    run_message_loop(window, &mut |event| {
        seen.push(event);
        if exit_on(event) { Flow::Exit } else { Flow::Continue }
    });
    seen
}

#[test]
fn what_a_window_reports_to_the_sink_reaches_the_loop_handler() {
    let window = MessageWindow::create().unwrap();
    window.sink().send(UiEvent::OverlayMenu { x: 3, y: 4 });

    let seen = loop_events(&window, |_| true);

    assert_eq!(seen, [UiEvent::OverlayMenu { x: 3, y: 4 }]);
}

#[test]
fn a_waker_on_another_thread_wakes_the_loop() {
    let window = MessageWindow::create().unwrap();
    let waker = window.waker();

    let seen = std::thread::scope(|scope| {
        scope.spawn(move || waker.wake());
        loop_events(&window, |event| event == UiEvent::Wake)
    });

    assert_eq!(seen, [UiEvent::Wake]);
}

#[test]
fn the_waker_can_move_between_threads_and_be_cloned() {
    fn assert_send_clone<T: Send + Clone>() {}
    assert_send_clone::<Waker>();
}

#[test]
fn a_restarted_taskbar_is_reported() {
    let window = MessageWindow::create().unwrap();
    // SAFETY: the window is alive on this thread; the message carries no data.
    unsafe { PostMessageW(Some(window.handle()), taskbar_created_message(), WPARAM(0), LPARAM(0)) }.unwrap();

    let seen = loop_events(&window, |_| true);

    assert_eq!(seen, [UiEvent::TaskbarRestarted]);
}

#[test]
fn the_loop_ends_when_the_thread_is_told_to_quit() {
    let window = MessageWindow::create().unwrap();
    // SAFETY: posts WM_QUIT to this thread's queue.
    unsafe { PostQuitMessage(0) };

    let seen = loop_events(&window, |_| false);

    assert!(seen.is_empty(), "no event was due: {seen:?}");
}

#[test]
fn a_tray_select_is_a_click_and_a_context_menu_opens_at_its_point() {
    let point = WPARAM(0x012C_FFFB);

    assert_eq!(
        tray_event(point, LPARAM(NIN_SELECT.cast_signed() as isize)),
        Some(UiEvent::TrayClick)
    );
    assert_eq!(
        tray_event(point, LPARAM(WM_CONTEXTMENU.cast_signed() as isize)),
        Some(UiEvent::TrayMenu { x: -5, y: 300 })
    );
}

#[test]
fn other_tray_notifications_are_ignored() {
    // A mouse-up also arrives with every click, next to NIN_SELECT; reacting to both would toggle twice.
    assert_eq!(tray_event(WPARAM(0), LPARAM(WM_LBUTTONUP.cast_signed() as isize)), None);
    assert_eq!(tray_event(WPARAM(0), LPARAM(WM_MOUSEMOVE.cast_signed() as isize)), None);
}

#[test]
fn a_message_can_be_let_through_to_a_window_but_not_to_a_window_that_is_gone() {
    let window = MessageWindow::create().unwrap();

    let allowed = allow_message(window.handle(), WM_APP + 2);
    let refused = allow_message(HWND::default(), WM_APP + 2);

    assert!(
        allowed.is_ok(),
        "a live window must accept the filter change: {allowed:?}"
    );
    assert!(refused.is_err(), "a missing window must report the failure");
}

#[test]
fn an_unregistered_message_is_never_the_taskbar_restart() {
    assert!(
        !is_taskbar_created(0, 0),
        "a failed registration (0) must not match WM_NULL"
    );
    assert!(is_taskbar_created(0xC123, 0xC123));
    assert!(!is_taskbar_created(0xC124, 0xC123));
}

#[test]
fn closing_the_message_window_ends_the_loop_instead_of_destroying_it() {
    let window = MessageWindow::create().unwrap();

    // SAFETY: the window is alive on this thread; WM_CLOSE carries no data.
    unsafe { SendMessageW(window.handle(), WM_CLOSE, None, None) };
    let seen = loop_events(&window, |_| false);

    assert!(seen.is_empty(), "no event was due: {seen:?}");
    // SAFETY: querying any handle is allowed.
    let alive = unsafe { IsWindow(Some(window.handle())) }.as_bool();
    assert!(alive, "the window must outlive a close request");
}

#[test]
fn a_tick_reaches_the_loop() {
    let window = MessageWindow::create().unwrap();
    window.set_tick(Duration::from_millis(10)).unwrap();

    let seen = loop_events(&window, |event| event == UiEvent::Tick);

    assert_eq!(seen, [UiEvent::Tick]);
}

#[test]
fn asking_to_end_the_session_is_agreed_to() {
    let window = MessageWindow::create().unwrap();

    // SAFETY: the window is alive on this thread; the message carries no data.
    let answer = unsafe { SendMessageW(window.handle(), WM_QUERYENDSESSION, None, None) };

    assert_eq!(answer.0, 1);
}

#[test]
fn an_ending_session_runs_the_hook_once_and_ends_the_loop() {
    let window = MessageWindow::create().unwrap();
    let runs = Rc::new(Cell::new(0));
    let counted = Rc::clone(&runs);
    window.on_session_end(move || counted.set(counted.get() + 1));

    // SAFETY: the window is alive on this thread; a non-zero wparam says the session is ending.
    unsafe { SendMessageW(window.handle(), WM_ENDSESSION, Some(WPARAM(1)), None) };
    let seen = loop_events(&window, |event| event == UiEvent::SessionEnding);

    assert_eq!(seen, [UiEvent::SessionEnding]);
    assert_eq!(runs.get(), 1);
}

#[test]
fn a_cancelled_session_end_does_nothing() {
    let window = MessageWindow::create().unwrap();
    let runs = Rc::new(Cell::new(0));
    let counted = Rc::clone(&runs);
    window.on_session_end(move || counted.set(counted.get() + 1));

    // SAFETY: the window is alive on this thread; a zero wparam says the end was cancelled.
    unsafe { SendMessageW(window.handle(), WM_ENDSESSION, Some(WPARAM(0)), None) };
    // SAFETY: posts WM_QUIT to this thread's queue, so the loop returns instead of waiting.
    unsafe { PostQuitMessage(0) };
    let seen = loop_events(&window, |_| false);

    assert!(seen.is_empty(), "no event was due: {seen:?}");
    assert_eq!(runs.get(), 0);
}
