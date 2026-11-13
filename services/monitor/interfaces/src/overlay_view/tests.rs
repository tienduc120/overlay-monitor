use std::time::Duration;

use monitor_application::{OverlayFrame, OverlayStyle, Settings, overlay_style};
use platform_win32::dpi::enable_per_monitor_awareness;
use platform_win32::overlay::OverlayWindow;
use platform_win32::text::TextStyle;
use platform_win32::ui::{EventSink, pump_messages_for};

use super::{OverlayView, dpi_changed, needs_redraw, text_style};

fn default_style() -> OverlayStyle {
    overlay_style(&Settings::default())
}

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

fn frame(lines: &[&str]) -> OverlayFrame {
    OverlayFrame {
        lines: lines.iter().map(|&l| String::from(l)).collect(),
    }
}

#[test]
fn the_first_frame_is_always_drawn() {
    assert!(needs_redraw(None, &frame(&["FPS: 30"]), &default_style()));
}

#[test]
fn the_same_lines_in_the_same_style_are_not_drawn_again() {
    let style = default_style();
    let last = frame(&["FPS: 30"]);

    assert!(!needs_redraw(Some((&last.lines, &style)), &frame(&["FPS: 30"]), &style));
}

#[test]
fn changed_text_is_drawn() {
    let style = default_style();
    let last = frame(&["FPS: 30"]);

    assert!(needs_redraw(Some((&last.lines, &style)), &frame(&["FPS: 31"]), &style));
}

#[test]
fn a_changed_style_is_drawn_even_with_the_same_text() {
    let style = default_style();
    let bigger = OverlayStyle {
        size_pt: style.size_pt + 1.0,
        ..style.clone()
    };
    let last = frame(&["FPS: 30"]);

    assert!(needs_redraw(Some((&last.lines, &style)), &frame(&["FPS: 30"]), &bigger));
}

#[test]
fn the_view_shows_and_hides_its_window() {
    let view = OverlayView::new(OverlayWindow::create(EventSink::discard()).unwrap());

    view.show();
    let shown = view.is_visible();
    view.hide();
    let hidden = !view.is_visible();

    assert!(shown && hidden, "shown: {shown}, hidden afterwards: {hidden}");
}

#[test]
fn a_new_dpi_means_the_window_must_be_drawn_again_even_for_the_same_text() {
    assert!(dpi_changed(96, 144), "a move to a scaled monitor must force a redraw");
    assert!(!dpi_changed(96, 96));
}
