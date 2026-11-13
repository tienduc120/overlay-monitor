//! Per-monitor DPI: the process sees every monitor in its own physical pixels, and sizes convert from points and
//! device-independent pixels (DIPs) to those pixels.

use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

use crate::Win32Error;

const DIPS_PER_INCH: f32 = 96.0;
const POINTS_PER_INCH: f32 = 72.0;

/// Makes the process per-monitor DPI aware (v2), so window and monitor rectangles are physical pixels on every
/// monitor, and a window hears `WM_DPICHANGED` when it moves to a monitor with another scale.
///
/// # Errors
/// The process's awareness was already set, by a manifest or an earlier call.
pub fn enable_per_monitor_awareness() -> Result<(), Win32Error> {
    // SAFETY: the context is a predefined constant, and the call only changes this process's awareness.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
}

/// A font size in points, in DIPs (1/96 inch).
pub fn dips_from_points(pt: f32) -> f32 {
    pt * DIPS_PER_INCH / POINTS_PER_INCH
}

/// A length in DIPs, in pixels on a monitor of `dpi` dots per inch.
pub fn pixels_from_dips(dips: f32, dpi: u32) -> f32 {
    // Every real DPI fits in a u16, which converts to f32 exactly.
    let dpi = f32::from(u16::try_from(dpi).unwrap_or(u16::MAX));
    dips * dpi / DIPS_PER_INCH
}

/// The width of a small icon (the tray's), in pixels at the system's DPI.
pub fn small_icon_size() -> u32 {
    // SAFETY: both calls only read system settings.
    let width = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    u32::try_from(width).unwrap_or(0)
}

#[cfg(test)]
mod tests;
