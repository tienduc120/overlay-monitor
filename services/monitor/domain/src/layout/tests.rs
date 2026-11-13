use jiff::Timestamp;

use super::Layout;
use crate::reading::{Sample, Snapshot};

fn at(s: i64) -> Timestamp {
    Timestamp::from_second(1_800_000_000 + s).unwrap()
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "snapshot fields are Option, so the helper reads at the call site"
)]
fn sample(value: f64, s: i64) -> Option<Sample> {
    Some(Sample { value, at: at(s) })
}

fn screenshot() -> Snapshot {
    Snapshot {
        fps: sample(30.0, 0),
        cpu_load: sample(2.0, 0),
        cpu_temp: sample(45.8, 0),
        gpu_load: sample(4.0, 0),
        gpu_temp: sample(56.0, 0),
        memory_load: sample(30.9, 0),
        memory_temp: sample(35.5, 0),
    }
}

#[test]
fn the_default_layout_renders_the_screenshot_lines() {
    let lines = Layout::default().render(&screenshot(), at(0));
    assert_eq!(
        lines,
        [
            "FPS:    30 FPS",
            "CPU:   2.0 %, 45.8 °C",
            "GPU:   4.0 %, 56.0 °C",
            "DDR5: 30.9 %, 35.5 °C"
        ]
    );
}

#[test]
fn a_missing_reading_shows_a_dash_in_its_column() {
    let snap = Snapshot {
        fps: None,
        cpu_temp: None,
        ..screenshot()
    };
    let lines = Layout::default().render(&snap, at(0));
    assert_eq!(lines[0], "FPS:     — FPS");
    assert_eq!(lines[1], "CPU:   2.0 %,    — °C");
}

#[test]
fn a_reading_older_than_three_intervals_shows_a_dash() {
    let snap = Snapshot {
        gpu_temp: sample(56.0, 0),
        ..screenshot()
    };
    assert_eq!(
        Layout::default().render(&snap, at(3))[2],
        "GPU:   4.0 %, 56.0 °C",
        "three intervals old is still fresh"
    );
    let stale = Layout::default().render(&snap, Timestamp::from_millisecond(at(3).as_millisecond() + 1).unwrap());
    assert_eq!(
        stale[2], "GPU:     — %,    — °C",
        "every reading here is now older than three intervals"
    );
}

#[test]
fn a_disabled_line_is_left_out_and_the_labels_realign() {
    let mut layout = Layout::default();
    layout.lines[3].enabled = false;
    assert_eq!(
        layout.render(&screenshot(), at(0)),
        ["FPS:   30 FPS", "CPU:  2.0 %, 45.8 °C", "GPU:  4.0 %, 56.0 °C"]
    );
}

#[test]
fn values_round_to_one_decimal_and_fps_to_a_whole_number() {
    let snap = Snapshot {
        fps: sample(143.6, 0),
        cpu_load: sample(99.96, 0),
        ..screenshot()
    };
    let lines = Layout::default().render(&snap, at(0));
    assert_eq!(lines[0], "FPS:   144 FPS");
    assert_eq!(lines[1], "CPU: 100.0 %, 45.8 °C");
}
