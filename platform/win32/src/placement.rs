//! Where the overlay sits, as a monitor's device name and an offset inside it, so a place survives a restart and a
//! changed monitor setup. The choice is plain numbers, testable without a desktop; the monitor list is Win32's.

use std::mem::size_of;

use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows::core::BOOL;

use crate::snap::Rect;

/// A monitor's device name (for example `\.\DISPLAY2`) and its full bounds in virtual-screen pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedMonitor {
    pub name: String,
    pub bounds: Rect,
}

/// The offset of the overlay's top-left from the top-left of the named monitor's full bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorPlacement {
    pub monitor: String,
    pub x: i32,
    pub y: i32,
}

/// Where the overlay's top-left goes: the saved monitor's top-left plus the offset, each coordinate kept inside that
/// monitor, which may be smaller than when the place was saved. A monitor that is not listed, or nothing saved, gives
/// `fallback`, so the overlay is never put off-screen.
pub fn resolve_origin(saved: Option<&MonitorPlacement>, monitors: &[NamedMonitor], fallback: (i32, i32)) -> (i32, i32) {
    let Some((saved, monitor)) = saved.and_then(|saved| {
        monitors
            .iter()
            .find(|monitor| monitor.name == saved.monitor)
            .map(|monitor| (saved, monitor))
    }) else {
        return fallback;
    };
    let bounds = monitor.bounds;
    (
        within(bounds.left, bounds.right, saved.x),
        within(bounds.top, bounds.bottom, saved.y),
    )
}

/// `start + offset`, held to the monitor's span from `start` to its last pixel.
fn within(start: i32, end: i32, offset: i32) -> i32 {
    let last = end.saturating_sub(1).max(start);
    start.saturating_add(offset).clamp(start, last)
}

/// Every monitor attached to the desktop now.
pub fn monitors() -> Vec<NamedMonitor> {
    let mut found: Vec<NamedMonitor> = Vec::new();
    // SAFETY: the callback only reads its arguments and the `Vec` that `data` points to, which outlives the
    // synchronous call; no clip or rectangle is passed. A failed enumeration leaves what was found so far.
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM((&raw mut found).expose_provenance().cast_signed()),
        )
    }
    .as_bool();
    found
}

unsafe extern "system" fn collect(monitor: HMONITOR, _dc: HDC, _area: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec` that `monitors` passed, alive and not otherwise touched during the enumeration.
    let found = unsafe { std::ptr::with_exposed_provenance_mut::<Vec<NamedMonitor>>(data.0.cast_unsigned()).as_mut() };
    if let Some(found) = found
        && let Some(named) = named_monitor(monitor)
    {
        found.push(named);
    }
    BOOL::from(true)
}

/// The device name and full bounds of `monitor`, or `None` when Windows cannot say.
pub(crate) fn named_monitor(monitor: HMONITOR) -> Option<NamedMonitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).unwrap_or(u32::MAX);
    // SAFETY: `info` is a live local with its size set; the extended struct starts with a `MONITORINFO`.
    unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast()) }
        .as_bool()
        .then(|| {
            let end = info
                .szDevice
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(info.szDevice.len());
            let rect = info.monitorInfo.rcMonitor;
            NamedMonitor {
                name: String::from_utf16_lossy(info.szDevice.get(..end).unwrap_or_default()),
                bounds: Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
            }
        })
}

#[cfg(test)]
mod tests;
