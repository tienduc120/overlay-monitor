//! The UI thread's plumbing: the hidden message window that tray callbacks and wake-ups arrive at, the loop that turns
//! them into [`UiEvent`]s, and where other windows report what the user did.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW,
    MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostMessageW, QS_ALLINPUT, RegisterClassExW,
    RegisterWindowMessageW, SetWindowLongPtrW, TranslateMessage, WM_APP, WM_CONTEXTMENU, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

use crate::Win32Error;
use crate::overlay::screen_point;

const CLASS: PCWSTR = w!("OverlayMonitorMessages");
/// Posted by a [`Waker`] when the sampler has a frame.
const WAKE: u32 = WM_APP + 1;
/// The keyboard's select on the tray icon: `NIN_SELECT` with the key flag, which the bindings do not name.
const SELECT_BY_KEY: u32 = NIN_SELECT | 0x1;
/// What the tray icon's callbacks arrive as.
pub(crate) const TRAY_CALLBACK: u32 = WM_APP + 2;

/// What the user did, for the UI thread's loop to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEvent {
    /// A left-click (or the keyboard's select) on the tray icon.
    TrayClick,
    /// A right-click on the tray icon at this screen point, where its menu opens.
    TrayMenu { x: i32, y: i32 },
    /// A right-click on the overlay at this screen point, where its menu opens.
    OverlayMenu { x: i32, y: i32 },
    /// The sampler thread has a new frame.
    Wake,
    /// Explorer restarted, so the tray icon is gone and must be added again.
    TaskbarRestarted,
}

/// What the loop's handler asks of it after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

/// The hidden window that receives tray callbacks and wake-ups. It belongs to the thread that created it, which must
/// run [`run_message_loop`] for it.
pub struct MessageWindow {
    window: HWND,
    /// Read by the window procedure through the window's user data, so it lives until the window is destroyed.
    queue: Rc<Queue>,
}

/// The events the window procedure and the sinks have collected for the loop's handler.
struct Queue {
    events: RefCell<VecDeque<UiEvent>>,
    /// The id Explorer broadcasts when its taskbar is created again.
    taskbar_created: u32,
}

impl Queue {
    fn push(&self, event: UiEvent) {
        self.events.borrow_mut().push_back(event);
    }

    fn pop(&self) -> Option<UiEvent> {
        self.events.borrow_mut().pop_front()
    }
}

/// Wakes the UI thread's loop from any thread.
#[derive(Clone, Debug)]
pub struct Waker {
    /// The window handle as an integer, as a raw handle is not `Send`.
    window: isize,
}

impl MessageWindow {
    /// Creates the window, hidden. It is a top-level window rather than a message-only one, because Explorer's
    /// taskbar restart is broadcast to top-level windows only.
    ///
    /// # Errors
    /// The window class or the window could not be created.
    pub fn create() -> Result<Self, Win32Error> {
        let instance = register_class()?;
        // SAFETY: the class is registered; there is no parent, menu, or creation data, and the window is never shown.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS,
                w!("Overlay Monitor messages"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )
        }?;
        let queue = Rc::new(Queue {
            events: RefCell::new(VecDeque::new()),
            taskbar_created: taskbar_created_message(),
        });
        // SAFETY: the window is ours. The pointer stays valid while the window exists: `Drop` destroys the window
        // before `queue` is freed. Until it is stored, the procedure finds 0 and leaves the state alone.
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                Rc::as_ptr(&queue).expose_provenance().cast_signed(),
            )
        };
        Ok(Self { window, queue })
    }

    /// A sink that delivers into this window's loop, for another window to report to.
    pub fn sink(&self) -> EventSink {
        let queue = Rc::clone(&self.queue);
        EventSink::new(move |event| queue.push(event))
    }

    pub fn waker(&self) -> Waker {
        Waker {
            window: self.window.0.expose_provenance().cast_signed(),
        }
    }

    pub(crate) fn handle(&self) -> HWND {
        self.window
    }
}

impl Drop for MessageWindow {
    fn drop(&mut self) {
        // SAFETY: the window was created on this thread, which owns it, and is destroyed once, here, before `queue`
        // is freed. A failed destroy leaves nothing to undo: the window goes when its thread ends.
        unsafe { DestroyWindow(self.window) }.ok();
    }
}

impl Waker {
    /// Asks the loop to deliver [`UiEvent::Wake`]. A window that is gone ignores it.
    pub fn wake(&self) {
        let window = HWND(ptr::with_exposed_provenance_mut(self.window.cast_unsigned()));
        // SAFETY: PostMessageW may be called from any thread, and an invalid or destroyed handle only makes it fail,
        // which is all a late wake-up deserves. The message carries no data.
        unsafe { PostMessageW(Some(window), WAKE, WPARAM(0), LPARAM(0)) }.ok();
    }
}

/// Runs the thread's message loop, handing each event to `handler`, until it answers `Exit` or the thread gets
/// `WM_QUIT`. The handler runs outside any window procedure, so it may show a menu or touch any window.
pub fn run_message_loop(window: &MessageWindow, handler: &mut dyn FnMut(UiEvent) -> Flow) {
    loop {
        while let Some(event) = window.queue.pop() {
            if handler(event) == Flow::Exit {
                return;
            }
        }
        let mut message = MSG::default();
        // SAFETY: `message` is a live local; with no window filter, every message of this thread is taken.
        let got = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
        // 0 is WM_QUIT and -1 a failure; neither leaves anything to dispatch.
        if got.0 <= 0 {
            return;
        }
        // SAFETY: `message` was just filled in by GetMessageW. TranslateMessage only says whether it posted a
        // character message, and DispatchMessageW returns the window procedure's answer; neither matters here.
        unsafe {
            TranslateMessage(&raw const message).as_bool();
            DispatchMessageW(&raw const message);
        }
    }
}

/// The message Explorer broadcasts after it restarts and its notification area is empty again.
pub(crate) fn taskbar_created_message() -> u32 {
    // SAFETY: the name is a static NUL-terminated string; the same name always gives the same id.
    unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }
}

/// What a tray callback means. With version 4 notifications the event is in the low word of `lparam` and the
/// anchor's screen point in `wparam`. A click also sends a mouse-up next to `NIN_SELECT`, so only the latter counts.
pub(crate) fn tray_event(wparam: WPARAM, lparam: LPARAM) -> Option<UiEvent> {
    let event = u32::try_from(lparam.0 & 0xFFFF).unwrap_or(0);
    match event {
        NIN_SELECT | SELECT_BY_KEY => Some(UiEvent::TrayClick),
        WM_CONTEXTMENU => {
            let (x, y) = screen_point(LPARAM(wparam.0.cast_signed()));
            Some(UiEvent::TrayMenu { x, y })
        }
        _ => None,
    }
}

fn register_class() -> Result<HINSTANCE, Win32Error> {
    // SAFETY: no name asks for this executable's own module.
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        lpfnWndProc: Some(window_procedure),
        hInstance: instance,
        lpszClassName: CLASS,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is a live local, and its name is static.
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        let error = Win32Error::from_thread();
        // A message window made earlier in this process registered it already.
        if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
            return Err(error);
        }
    }
    Ok(instance)
}

/// Turns the messages this window cares about into queued events; the rest get the default handling.
///
/// # Safety
/// Only Windows calls this, with the arguments of a message for a window of this class.
unsafe extern "system" fn window_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: reading a window's user data has no preconditions.
    let data = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    let state = ptr::with_exposed_provenance::<Queue>(data.cast_unsigned());
    // SAFETY: non-zero data is the `Rc<Queue>` pointer `create` stored, which `MessageWindow` keeps alive until it has
    // destroyed the window; only shared references are ever made from it.
    if let Some(queue) = unsafe { state.as_ref() } {
        let event = match message {
            TRAY_CALLBACK => tray_event(wparam, lparam),
            WAKE => Some(UiEvent::Wake),
            other if other == queue.taskbar_created => Some(UiEvent::TaskbarRestarted),
            _ => None,
        };
        if let Some(event) = event {
            queue.push(event);
            return LRESULT(0);
        }
    }
    // SAFETY: the arguments are the ones Windows passed.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Where a window reports what the user did to it.
pub struct EventSink {
    deliver: Box<dyn Fn(UiEvent)>,
}

impl EventSink {
    /// A sink that drops every event, for a window nobody listens to.
    pub fn discard() -> Self {
        Self::new(|_event| {})
    }

    /// A sink that hands each event to `deliver`.
    pub(crate) fn new(deliver: impl Fn(UiEvent) + 'static) -> Self {
        Self {
            deliver: Box::new(deliver),
        }
    }

    pub(crate) fn send(&self, event: UiEvent) {
        (self.deliver)(event);
    }
}

/// Dispatches this thread's messages to their windows until `duration` has passed, sleeping while there are none.
pub fn pump_messages_for(duration: Duration) {
    let start = Instant::now();
    loop {
        dispatch_waiting_messages();
        let left = duration.saturating_sub(start.elapsed());
        if left.is_zero() {
            break;
        }
        // Rounded up, so a wait never ends just short of the deadline and spins; one below INFINITE, so it ends.
        let millis = u32::try_from(left.as_micros().div_ceil(1000)).unwrap_or(INFINITE - 1);
        // SAFETY: no handles are passed; the call waits for input to this thread or the timeout. Either way the loop
        // looks again, so the result does not matter.
        unsafe { MsgWaitForMultipleObjects(None, false, millis, QS_ALLINPUT) };
    }
}

fn dispatch_waiting_messages() {
    let mut message = MSG::default();
    // SAFETY: `message` is a live local; with no window filter, every message of this thread is taken.
    while unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
        // SAFETY: `message` was just filled in by PeekMessageW. TranslateMessage only says whether it posted a
        // character message, and DispatchMessageW returns the window procedure's answer; neither matters here.
        unsafe {
            TranslateMessage(&raw const message).as_bool();
            DispatchMessageW(&raw const message);
        }
    }
}

#[cfg(test)]
mod tests;
