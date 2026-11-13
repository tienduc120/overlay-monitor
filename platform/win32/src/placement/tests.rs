use super::{MonitorPlacement, NamedMonitor, resolve_origin};
use crate::snap::Rect;

fn monitors() -> Vec<NamedMonitor> {
    vec![
        NamedMonitor {
            name: String::from(r"\.\DISPLAY1"),
            bounds: Rect {
                left: 0,
                top: 0,
                right: 2560,
                bottom: 1440,
            },
        },
        NamedMonitor {
            name: String::from(r"\.\DISPLAY2"),
            bounds: Rect {
                left: 2560,
                top: 0,
                right: 4480,
                bottom: 1080,
            },
        },
    ]
}

fn saved(monitor: &str, x: i32, y: i32) -> MonitorPlacement {
    MonitorPlacement {
        monitor: String::from(monitor),
        x,
        y,
    }
}

#[test]
fn a_saved_monitor_that_is_present_places_the_overlay_at_its_offset() {
    let at = resolve_origin(Some(&saved(r"\.\DISPLAY2", 100, 50)), &monitors(), (8, 8));

    assert_eq!(at, (2660, 50));
}

#[test]
fn a_monitor_that_is_gone_falls_back_to_the_main_monitors_top_left() {
    let at = resolve_origin(Some(&saved(r"\.\DISPLAY3", 100, 50)), &monitors(), (8, 8));

    assert_eq!(at, (8, 8));
}

#[test]
fn nothing_saved_falls_back_too() {
    assert_eq!(resolve_origin(None, &monitors(), (8, 8)), (8, 8));
}

#[test]
fn an_offset_past_a_smaller_monitor_is_pulled_back_inside() {
    let at = resolve_origin(Some(&saved(r"\.\DISPLAY2", 2500, 1400)), &monitors(), (8, 8));

    assert_eq!(at, (4479, 1079));
}

#[test]
fn a_negative_offset_is_pulled_back_to_the_monitors_edge() {
    let at = resolve_origin(Some(&saved(r"\.\DISPLAY2", -40, -5)), &monitors(), (8, 8));

    assert_eq!(at, (2560, 0));
}
