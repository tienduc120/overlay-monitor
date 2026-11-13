//! The UI thread's plumbing: where windows report what the user did, and pumping the thread's messages.

use std::time::{Duration, Instant};

use windows::Win32::System::Threading::INFINITE;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage,
};

/// What the user did, for the UI thread's loop to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEvent {
    /// A right-click on the overlay at this screen point, where its menu opens.
    OverlayMenu { x: i32, y: i32 },
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
