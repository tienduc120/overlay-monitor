use jiff::{SignedDuration, Timestamp};

use super::{Field, Layout, MemoryKind, clamp_interval};
use crate::reading::{Sample, Snapshot, Sticks};

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
        ..Snapshot::default()
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

fn only(line: usize, fields: &[Field]) -> Layout {
    let mut layout = Layout::default();
    layout.lines[line].fields = fields.to_vec();
    layout
}

#[test]
fn a_machine_without_ddr5_labels_the_memory_line_ram() {
    assert_eq!(Layout::for_memory(MemoryKind::Other).lines[3].label, "RAM");
    assert_eq!(Layout::for_memory(MemoryKind::Ddr5).lines[3].label, "DDR5");
    assert_eq!(Layout::default(), Layout::for_memory(MemoryKind::Ddr5));
}

#[test]
fn the_interval_stays_between_a_quarter_second_and_five_seconds() {
    let ms = SignedDuration::from_millis;
    assert_eq!(clamp_interval(ms(100)), ms(250));
    assert_eq!(
        clamp_interval(SignedDuration::from_secs(10)),
        SignedDuration::from_secs(5)
    );
    assert_eq!(
        clamp_interval(SignedDuration::from_secs(1)),
        SignedDuration::from_secs(1)
    );
    assert_eq!(clamp_interval(SignedDuration::from_secs(-1)), ms(250));
}

#[test]
fn the_fps_line_can_add_frame_time_and_the_worst_frame() {
    let snap = Snapshot {
        worst_frame_ms: sample(41.2, 0),
        ..screenshot()
    };
    let layout = only(0, &[Field::Fps, Field::FrameTime, Field::WorstFrameTime]);
    assert_eq!(layout.render(&snap, at(0))[0], "FPS:    30 FPS,  33.3 ms, max  41.2 ms");
}

#[test]
fn extra_values_follow_their_line_in_a_fixed_order() {
    let snap = Snapshot {
        gpu_fan_rpm: sample(1200.0, 0),
        gpu_memory_mhz: sample(644.0, 0),
        gpu_video_load: sample(12.5, 0),
        ..screenshot()
    };
    let layout = only(
        2,
        &[
            Field::VideoLoad,
            Field::Fan,
            Field::Load,
            Field::Temperature,
            Field::MemoryClock,
        ],
    );
    assert_eq!(
        layout.render(&snap, at(0))[2],
        "GPU:   4.0 %, 56.0 °C, 1200 rpm,  644 MHz, video  12.5 %"
    );
}

#[test]
fn each_stick_reads_in_address_order() {
    let all = [Field::Load, Field::Used, Field::Temperature, Field::Sticks];
    let snap = Snapshot {
        memory_used_gb: sample(9.9, 0),
        memory_sticks: Some(Sticks {
            celsius: alloc::vec![36.3, 33.3],
            at: at(0),
        }),
        ..screenshot()
    };
    let layout = only(3, &all);
    assert_eq!(
        layout.render(&snap, at(0))[3],
        "DDR5: 30.9 %,  9.9 GB, 35.5 °C, 36.3/33.3 °C"
    );
    let without = Snapshot {
        memory_sticks: None,
        ..snap
    };
    assert_eq!(
        layout.render(&without, at(0))[3],
        "DDR5: 30.9 %,  9.9 GB, 35.5 °C,    — °C"
    );
}

#[test]
fn every_value_and_its_dash_take_the_same_width() {
    let full = Snapshot {
        worst_frame_ms: sample(41.2, 0),
        gpu_fan_rpm: sample(1200.0, 0),
        gpu_memory_mhz: sample(644.0, 0),
        gpu_video_load: sample(12.5, 0),
        memory_used_gb: sample(9.9, 0),
        ..screenshot()
    };
    let cases = [
        (0, Field::Fps),
        (0, Field::FrameTime),
        (0, Field::WorstFrameTime),
        (1, Field::Load),
        (1, Field::Temperature),
        (2, Field::Fan),
        (2, Field::MemoryClock),
        (2, Field::VideoLoad),
        (3, Field::Used),
    ];
    for (line, field) in cases {
        let layout = only(line, &[field]);
        let with = layout.render(&full, at(0))[0].chars().count();
        let none = layout.render(&Snapshot::default(), at(0))[0].chars().count();
        assert_eq!(with, none, "{field:?}");
    }
}

#[test]
fn an_extra_value_older_than_three_intervals_shows_a_dash() {
    let snap = Snapshot {
        gpu_fan_rpm: sample(1200.0, 0),
        ..screenshot()
    };
    let layout = only(2, &[Field::Fan]);
    let later = Timestamp::from_millisecond(at(3).as_millisecond() + 1).unwrap();
    assert_eq!(layout.render(&snap, at(3))[2], "GPU: 1200 rpm");
    assert_eq!(layout.render(&snap, later)[2], "GPU:    — rpm");
}

#[test]
fn a_line_with_no_value_ticked_is_left_out() {
    let layout = only(1, &[]);
    assert_eq!(
        layout.render(&screenshot(), at(0)),
        ["FPS:    30 FPS", "GPU:   4.0 %, 56.0 °C", "DDR5: 30.9 %, 35.5 °C"]
    );
}

#[test]
fn a_field_the_line_cannot_show_is_ignored() {
    let layout = only(1, &[Field::Load, Field::Temperature, Field::Fan]);
    assert_eq!(layout.render(&screenshot(), at(0))[0], "FPS:    30 FPS");
    assert_eq!(layout.render(&screenshot(), at(0))[1], "CPU:   2.0 %, 45.8 °C");
}
