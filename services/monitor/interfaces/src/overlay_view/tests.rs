use std::time::Duration;

use monitor_application::{OverlayFrame, OverlayStyle, default_style};
use platform_win32::dpi::enable_per_monitor_awareness;
use platform_win32::overlay::OverlayWindow;
use platform_win32::text::TextStyle;
use platform_win32::ui::{EventSink, pump_messages_for};

use super::{OverlayView, text_style};

#[test]
fn the_text_style_carries_every_overlay_style_value() {
    let s = default_style();
    let t = text_style(&s);
    assert_eq!(
        (t.font_family.as_str(), t.text_rgb, t.outline_px, t.background_alpha),
        ("Consolas", 0x00_FF_00, 2.0, 1)
    );
}

#[test]
fn each_value_lands_in_its_own_field() {
    let style = OverlayStyle {
        font_family: String::from("Cascadia Mono"),
        size_pt: 11.5,
        bold: false,
        italic: true,
        text_rgb: 0x11_11_11,
        outline_rgb: 0x22_22_22,
        outline_px: 3.0,
        background_rgb: 0x33_33_33,
        background_alpha: 44,
    };

    let text = text_style(&style);

    assert_eq!(
        text,
        TextStyle {
            font_family: String::from("Cascadia Mono"),
            size_pt: 11.5,
            bold: false,
            italic: true,
            text_rgb: 0x11_11_11,
            outline_rgb: 0x22_22_22,
            outline_px: 3.0,
            background_rgb: 0x33_33_33,
            background_alpha: 44,
        }
    );
}

/// The agent's visual check: run with `--run-ignored only` and capture the screen while it is up.
#[test]
#[ignore = "opens a window on the desktop"]
fn shows_the_screenshot_lines_for_three_seconds() {
    enable_per_monitor_awareness().unwrap();
    let window = OverlayWindow::create(EventSink::discard()).unwrap();
    window.show();
    let mut view = OverlayView::new(window);
    let frame = OverlayFrame {
        lines: [
            "FPS:   30 FPS",
            "CPU:   2.0 %, 45.8 °C",
            "GPU:   4.0 %, 56.0 °C",
            "DDR5: 30.9 %, 35.5 °C",
        ]
        .map(String::from)
        .to_vec(),
    };

    view.show_frame(&frame, &default_style()).unwrap();
    pump_messages_for(Duration::from_secs(3));
}
