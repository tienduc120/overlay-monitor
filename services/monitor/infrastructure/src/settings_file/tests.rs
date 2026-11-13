use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use monitor_application::{
    Field, Line, LineSettings, LoadSettingsError, OverlayPosition, Settings, SettingsStore, ShortcutKeys,
    StyleSettings, load_settings,
};
use monitor_domain::MemoryKind;

use super::{TomlSettingsStore, settings_path};

/// A folder of the test's own, removed when the test ends, even by a failed assertion.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("overlay-monitor-{test}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        Self(dir)
    }

    fn file(&self) -> PathBuf {
        self.0.join("settings.toml")
    }

    fn store_with(&self, text: &str) -> TomlSettingsStore {
        fs::create_dir_all(&self.0).expect("create the folder");
        fs::write(self.file(), text).expect("write the file");
        TomlSettingsStore::new(self.file())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn loaded(text: &str, name: &str) -> Settings {
    let scratch = Scratch::new(name);
    load_settings(&scratch.store_with(text), MemoryKind::Ddr5)
}

fn line(settings: &Settings, which: Line) -> &LineSettings {
    settings
        .lines
        .iter()
        .find(|l| l.line == which)
        .expect("every line is present")
}

fn customised() -> Settings {
    let defaults = Settings::default();
    let mut settings = Settings {
        update_interval: Duration::from_millis(2500),
        gpu: Some(String::from("AMD Radeon RX 9060 XT")),
        show_hide: ShortcutKeys::parse("Ctrl+Alt+F9").expect("a usable shortcut"),
        lock_unlock: ShortcutKeys::parse("Alt+Shift+L").expect("a usable shortcut"),
        start_with_windows: true,
        visible: false,
        locked: true,
        style: StyleSettings {
            font_family: String::from("Segoe UI"),
            size_pt: 12.3,
            bold: false,
            italic: true,
            text_rgb: 0x12_34_56,
            outline_rgb: 0xAB_CD_EF,
            outline_px: 1.5,
            background_rgb: 0x01_02_03,
            background_opacity_percent: 40,
        },
        ..defaults
    };
    settings.lines = vec![
        LineSettings {
            line: Line::Fps,
            label: String::from("Frames"),
            enabled: false,
            fields: vec![Field::FrameTime],
        },
        LineSettings {
            line: Line::Cpu,
            label: String::from("Proc"),
            enabled: true,
            fields: vec![Field::Temperature],
        },
        LineSettings {
            line: Line::Gpu,
            label: String::from("Card"),
            enabled: true,
            fields: vec![Field::Load, Field::Fan, Field::MemoryClock, Field::VideoLoad],
        },
        LineSettings {
            line: Line::Memory,
            label: String::from("RAM"),
            enabled: false,
            fields: vec![Field::Used, Field::Sticks],
        },
    ];
    settings.position = Some(OverlayPosition {
        monitor: String::from(r"\\.\DISPLAY2"),
        x: -30,
        y: 45,
    });
    settings
}

#[test]
fn the_path_is_in_the_overlay_monitor_folder() {
    let path = settings_path(Path::new(r"C:\Users\me\AppData\Roaming"));
    assert_eq!(
        path,
        Path::new(r"C:\Users\me\AppData\Roaming\OverlayMonitor\settings.toml")
    );
}

#[test]
fn settings_saved_then_loaded_come_back_the_same() {
    let scratch = Scratch::new("round-trip");
    let store = TomlSettingsStore::new(scratch.file());
    let settings = customised();

    store.save(&settings).expect("save");

    assert_eq!(store.load().expect("load"), Some(settings));
}

#[test]
fn a_missing_file_is_none_not_an_error() {
    let scratch = Scratch::new("missing");
    assert_eq!(TomlSettingsStore::new(scratch.file()).load().expect("load"), None);
}

#[test]
fn saving_creates_the_folder() {
    let scratch = Scratch::new("creates-folder");
    let nested = scratch.0.join("nested");
    TomlSettingsStore::new(nested.join("settings.toml"))
        .save(&Settings::default())
        .expect("save");
    assert!(nested.join("settings.toml").is_file());
    assert!(!nested.join("settings.toml.tmp").exists());
}

#[test]
fn a_file_that_is_not_toml_is_kept_as_bad_and_reported() {
    let scratch = Scratch::new("not-toml");
    let store = scratch.store_with("this is [not toml");
    let error = store.load().expect_err("unreadable");
    let LoadSettingsError::Unreadable { kept_as, .. } = error else {
        panic!("expected Unreadable, got {error:?}");
    };
    assert!(kept_as.ends_with("settings.toml.bad"), "{kept_as}");
    assert_eq!(
        fs::read(scratch.0.join("settings.toml.bad")).expect("the copy"),
        b"this is [not toml"
    );
}

#[test]
fn a_value_of_the_wrong_type_makes_the_file_unreadable() {
    let scratch = Scratch::new("wrong-type");
    let store = scratch.store_with("[style]\nsize_pt = \"big\"\n");
    assert!(matches!(store.load(), Err(LoadSettingsError::Unreadable { .. })));
}

#[test]
fn a_bad_colour_or_shortcut_falls_back_and_the_rest_is_kept() {
    let settings = loaded(
        "visible = false\nshow_hide = \"Shift+O\"\n[style]\ntext = \"#GG0000\"\n",
        "bad-values",
    );
    assert!(!settings.visible);
    assert_eq!(settings.show_hide, Settings::default().show_hide);
    assert_eq!(settings.style.text_rgb, 0x00_FF_00);
}

#[test]
fn keys_left_out_take_their_defaults() {
    assert_eq!(
        loaded("locked = true\n", "left-out"),
        Settings {
            locked: true,
            ..Settings::default()
        }
    );
}

#[test]
fn numbers_of_any_size_or_kind_are_clamped_not_rejected() {
    let settings = loaded(
        "update_seconds = 1000000\n[style]\nsize_pt = 14\nbackground_opacity_percent = 300\n\
         [position]\nmonitor = \"M\"\nx = 1000000000000\ny = -2.5e15\n",
        "big-numbers",
    );
    assert_eq!(settings.style.size_pt.to_bits(), 14.0_f32.to_bits());
    assert_eq!(settings.style.background_opacity_percent, 100);
    assert_eq!(settings.update_interval, Duration::from_secs(5));
    let position = settings.position.expect("a position");
    assert_eq!((position.x, position.y), (i32::MAX, i32::MIN));
}

#[test]
fn the_default_file_reads_as_documented() {
    let scratch = Scratch::new("documented");
    TomlSettingsStore::new(scratch.file())
        .save(&Settings::default())
        .expect("save");
    let text = fs::read_to_string(scratch.file()).expect("read");
    let header = "# Overlay Monitor settings. The app rewrites this file when a setting changes, so edit it while the app is closed.";
    assert!(text.starts_with(header), "{text}");
    for expected in [
        "update_seconds = 1.0",
        "show_hide = \"Ctrl+Shift+O\"",
        "text = \"#00FF00\"",
        "[lines.memory]",
        "label = \"DDR5\"",
        "fan = false",
        "sticks = false",
    ] {
        assert!(text.contains(expected), "missing {expected} in\n{text}");
    }
    assert!(!text.contains("gpu ="), "{text}");
    assert!(!text.contains("[position]"), "{text}");
}

#[test]
fn value_keys_left_out_take_the_lines_defaults() {
    let settings = loaded("[lines.gpu]\nfan = true\n[lines.cpu]\nfan = true\n", "value-defaults");
    assert_eq!(
        line(&settings, Line::Gpu).fields,
        [Field::Load, Field::Temperature, Field::Fan]
    );
    assert_eq!(line(&settings, Line::Cpu).fields, [Field::Load, Field::Temperature]);
}
