use std::time::Duration;

use monitor_domain::MemoryKind;

use super::{
    Field, Line, LineSettings, Settings, ShortcutKeys, StyleSettings, blank_frame, layout_of, load_settings,
    overlay_style,
};
use crate::testing::MemoryStore;

fn line(line: Line, label: &str, fields: &[Field]) -> LineSettings {
    LineSettings {
        line,
        label: String::from(label),
        enabled: true,
        fields: fields.to_vec(),
    }
}

fn keys(modifiers: &str, key: u8) -> ShortcutKeys {
    ShortcutKeys {
        ctrl: modifiers.contains('c'),
        shift: modifiers.contains('s'),
        alt: modifiers.contains('a'),
        key,
    }
}

fn assert_default_style(s: &Settings) {
    assert_eq!(s.style.font_family, "Consolas");
    assert!((s.style.size_pt - 14.0).abs() < f32::EPSILON);
    assert!(s.style.bold);
    assert!(!s.style.italic);
    assert_eq!(s.style.text_rgb, 0x0000_FF00);
    assert_eq!(s.style.outline_rgb, 0);
    assert!((s.style.outline_px - 2.0).abs() < f32::EPSILON);
    assert_eq!(s.style.background_rgb, 0);
    assert_eq!(s.style.background_opacity_percent, 0);
}

#[test]
fn the_defaults_are_the_specs() {
    let s = Settings::default();
    assert_default_style(&s);
    let labels: Vec<_> = s.lines.iter().map(|l| (l.line, l.label.as_str(), l.enabled)).collect();
    assert_eq!(
        labels,
        [
            (Line::Fps, "FPS", true),
            (Line::Cpu, "CPU", true),
            (Line::Gpu, "GPU", true),
            (Line::Memory, "DDR5", true)
        ]
    );
    assert_eq!(s.update_interval, Duration::from_secs(1));
    assert_eq!(s.gpu, None);
    assert_eq!(s.show_hide.to_string(), "Ctrl+Shift+O");
    assert_eq!(s.lock_unlock.to_string(), "Ctrl+Shift+L");
    assert!(!s.start_with_windows);
    assert!(s.visible);
    assert!(!s.locked);
    assert_eq!(s.position, None);
}

#[test]
fn on_a_machine_without_ddr5_the_memory_label_defaults_to_ram() {
    assert_eq!(Settings::defaults(MemoryKind::Other).lines[3].label, "RAM");
}

#[test]
fn normalizing_fills_missing_lines_and_empty_labels() {
    let s = Settings {
        lines: vec![line(Line::Cpu, "  ", &[Field::Load])],
        ..Settings::default()
    }
    .normalized(MemoryKind::Ddr5);
    let order: Vec<_> = s.lines.iter().map(|l| l.line).collect();
    assert_eq!(order, [Line::Fps, Line::Cpu, Line::Gpu, Line::Memory]);
    assert_eq!(s.lines[1].label, "CPU");
}

#[test]
fn the_default_fields_are_todays_four_lines() {
    let fields: Vec<_> = Settings::default().lines.into_iter().map(|l| l.fields).collect();
    assert_eq!(fields[0], [Field::Fps]);
    assert_eq!(fields[1], [Field::Load, Field::Temperature]);
    assert_eq!(fields[2], [Field::Load, Field::Temperature]);
    assert_eq!(fields[3], [Field::Load, Field::Temperature]);
}

#[test]
fn normalizing_keeps_only_the_fields_a_line_can_show() {
    let s = Settings {
        lines: vec![
            line(Line::Cpu, "CPU", &[Field::Fan, Field::Load]),
            line(Line::Gpu, "GPU", &[Field::Fan, Field::Load, Field::Fan]),
        ],
        ..Settings::default()
    }
    .normalized(MemoryKind::Ddr5);
    assert_eq!(s.lines[1].fields, [Field::Load]);
    assert_eq!(s.lines[2].fields, [Field::Load, Field::Fan]);
}

#[test]
fn normalizing_clamps_the_interval_and_the_style() {
    let s = Settings {
        update_interval: Duration::from_secs(30),
        style: StyleSettings {
            size_pt: 200.0,
            ..Settings::default().style
        },
        ..Settings::default()
    }
    .normalized(MemoryKind::Ddr5);
    assert_eq!(s.update_interval, Duration::from_secs(5));
    assert!((s.style.size_pt - 72.0).abs() < f32::EPSILON);
}

#[test]
fn an_invalid_or_repeated_shortcut_falls_back_to_the_defaults() {
    let s = Settings {
        show_hide: keys("cs", 0x20),
        ..Settings::default()
    }
    .normalized(MemoryKind::Ddr5);
    assert_eq!(s.show_hide.to_string(), "Ctrl+Shift+O");

    let s = Settings {
        show_hide: keys("ca", b'K'),
        lock_unlock: keys("ca", b'K'),
        ..Settings::default()
    }
    .normalized(MemoryKind::Ddr5);
    assert_eq!(s.show_hide.to_string(), "Ctrl+Shift+O");
    assert_eq!(s.lock_unlock.to_string(), "Ctrl+Shift+L");
}

#[test]
fn an_empty_gpu_or_monitor_name_means_none() {
    let s = Settings {
        gpu: Some(String::new()),
        position: Some(super::OverlayPosition {
            monitor: String::new(),
            x: 1,
            y: 2,
        }),
        ..Settings::default()
    }
    .normalized(MemoryKind::Ddr5);
    assert_eq!((s.gpu, s.position), (None, None));
}

#[test]
fn the_sampler_layout_carries_each_lines_fields() {
    let layout = layout_of(&Settings::default());
    assert_eq!(layout, monitor_domain::Layout::default());
}

#[test]
fn a_missing_file_loads_the_defaults() {
    assert_eq!(
        load_settings(&MemoryStore::empty(), MemoryKind::Ddr5),
        Settings::default()
    );
}

#[test]
fn an_unreadable_file_loads_the_defaults() {
    assert_eq!(
        load_settings(&MemoryStore::unreadable(), MemoryKind::Other),
        Settings::defaults(MemoryKind::Other)
    );
}

#[test]
fn a_stored_file_is_normalized_on_load() {
    let stored = Settings {
        update_interval: Duration::from_secs(30),
        ..Settings::default()
    };
    let loaded = load_settings(&MemoryStore::holding(stored), MemoryKind::Ddr5);
    assert_eq!(loaded.update_interval, Duration::from_secs(5));
}

#[test]
fn a_locked_overlay_may_be_fully_see_through() {
    let locked = Settings {
        locked: true,
        ..Settings::default()
    };
    assert_eq!(overlay_style(&locked).background_alpha, 0);
    assert_eq!(overlay_style(&Settings::default()).background_alpha, 1);
}

#[test]
fn shortcut_keys_parse_and_print_like_the_settings_file() {
    assert_eq!(ShortcutKeys::parse("Ctrl+Shift+O").unwrap(), keys("cs", b'O'));
    let err = ShortcutKeys::parse("Shift+O").unwrap_err();
    assert!(err.to_string().contains("Shift+O"));
    assert!(keys("c", b'A').is_valid());
    assert!(!keys("s", b'A').is_valid());
}

#[test]
fn the_store_fake_records_every_save() {
    use crate::SettingsStore;
    let store = MemoryStore::empty();
    let handle = store.clone();
    store.save(&Settings::default()).unwrap();
    assert_eq!(handle.saved().len(), 1);
    assert!(MemoryStore::failing_saves().save(&Settings::default()).is_err());
}

#[test]
fn a_blank_frame_shows_every_enabled_line_with_dashes() {
    let frame = blank_frame(&Settings::defaults(MemoryKind::Ddr5));

    assert_eq!(
        frame.lines,
        [
            "FPS:     — FPS",
            "CPU:     — %,    — °C",
            "GPU:     — %,    — °C",
            "DDR5:    — %,    — °C"
        ]
    );
}
