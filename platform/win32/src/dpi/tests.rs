use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetThreadDpiAwarenessContext,
};

use super::{dips_from_points, enable_per_monitor_awareness, pixels_from_dips};

#[test]
fn fourteen_points_is_about_18_7_dips_and_scales_with_dpi() {
    assert!((dips_from_points(14.0) - 18.666_667).abs() < 1e-4);
    assert!((pixels_from_dips(18.666_667, 144) - 28.0).abs() < 1e-3, "150 % scaling");
}

#[test]
fn once_enabled_the_process_sees_each_monitor_in_its_own_pixels() {
    enable_per_monitor_awareness().unwrap();

    // SAFETY: both calls only read and compare awareness context handles.
    let per_monitor = unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    };

    assert!(per_monitor.as_bool(), "the UI thread must run per-monitor aware (v2)");
}
