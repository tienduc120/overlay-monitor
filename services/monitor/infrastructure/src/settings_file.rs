//! The settings as a TOML file in the user's app data (ADR-0018).
//!
//! The file is edited by hand, so reading is forgiving: every key is optional, a string that is not a colour or a
//! shortcut takes its default, and numbers are clamped. Only a value of the wrong TOML type, or text that is not TOML,
//! makes the file unreadable.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use monitor_application::{
    Field, Line, LineSettings, LoadSettingsError, OverlayPosition, SaveSettingsError, Settings, SettingsStore,
    ShortcutKeys, StyleSettings, line_fields,
};
use monitor_domain::Rgb;
use serde::{Deserialize, Serialize};

const HEADER: &str = "# Overlay Monitor settings. The app rewrites this file when a setting changes, so edit it while the app is closed.\n";

/// `<roaming app data>\OverlayMonitor\settings.toml`.
pub fn settings_path(roaming_app_data: &Path) -> PathBuf {
    roaming_app_data.join("OverlayMonitor").join("settings.toml")
}

/// A number as written: a whole number or a fractional one, so neither is a type error where the other is meant.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Number {
    Whole(i64),
    Fractional(f64),
}

/// Formatting rounds without a cast; what does not fit an `i64` saturates by sign, and NaN is zero.
fn round_saturating(value: f64) -> i64 {
    format!("{value:.0}").parse().unwrap_or_else(|_| {
        if value.is_nan() {
            0
        } else if value < 0.0 {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

fn saturating_i32(value: i64) -> i32 {
    i32::try_from(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or_default()
}

impl Number {
    fn whole(self) -> i64 {
        match self {
            Self::Whole(value) => value,
            Self::Fractional(value) => round_saturating(value),
        }
    }

    fn float(self) -> f64 {
        match self {
            Self::Whole(value) => f64::from(saturating_i32(value)),
            Self::Fractional(value) => value,
        }
    }

    /// The `f32` nearest the number, or `default` for NaN; the text round trip avoids a narrowing cast.
    fn single(self, default: f32) -> f32 {
        self.float()
            .to_string()
            .parse()
            .ok()
            .filter(|v: &f32| !v.is_nan())
            .unwrap_or(default)
    }
}

/// An `f32` as the `f64` that prints the same, so `12.3` is written as `12.3`.
fn widen(value: f32) -> f64 {
    value.to_string().parse().unwrap_or_default()
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SettingsFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    update_seconds: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    show_hide: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock_unlock: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_with_windows: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpu: Option<String>,
    style: StyleFile,
    lines: LinesFile,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<PositionFile>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct StyleFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_pt: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outline_px: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background_opacity_percent: Option<Number>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct LinesFile {
    fps: LineFile,
    cpu: LineFile,
    gpu: LineFile,
    memory: LineFile,
}

impl LinesFile {
    const fn line(&self, line: Line) -> &LineFile {
        match line {
            Line::Fps => &self.fps,
            Line::Cpu => &self.cpu,
            Line::Gpu => &self.gpu,
            Line::Memory => &self.memory,
        }
    }

    const fn line_mut(&mut self, line: Line) -> &mut LineFile {
        match line {
            Line::Fps => &mut self.fps,
            Line::Cpu => &mut self.cpu,
            Line::Gpu => &mut self.gpu,
            Line::Memory => &mut self.memory,
        }
    }
}

/// Every key any line can have; a line reads and writes only the ones it shows.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct LineFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fps: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worst_frame_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fan: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vram_clock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    used: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sticks: Option<bool>,
}

impl LineFile {
    const fn slot(&mut self, field: Field) -> &mut Option<bool> {
        match field {
            Field::Fps => &mut self.fps,
            Field::FrameTime => &mut self.frame_time,
            Field::WorstFrameTime => &mut self.worst_frame_time,
            Field::Load => &mut self.load,
            Field::Temperature => &mut self.temperature,
            Field::Fan => &mut self.fan,
            Field::MemoryClock => &mut self.vram_clock,
            Field::VideoLoad => &mut self.video,
            Field::Used => &mut self.used,
            Field::Sticks => &mut self.sticks,
        }
    }

    const fn value(&self, field: Field) -> Option<bool> {
        match field {
            Field::Fps => self.fps,
            Field::FrameTime => self.frame_time,
            Field::WorstFrameTime => self.worst_frame_time,
            Field::Load => self.load,
            Field::Temperature => self.temperature,
            Field::Fan => self.fan,
            Field::MemoryClock => self.vram_clock,
            Field::VideoLoad => self.video,
            Field::Used => self.used,
            Field::Sticks => self.sticks,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct PositionFile {
    monitor: Option<String>,
    x: Option<Number>,
    y: Option<Number>,
}

fn colour(text: Option<&str>, default: u32) -> u32 {
    text.and_then(Rgb::parse_hex).map_or(default, |rgb| rgb.0)
}

fn shortcut(text: Option<&str>, default: ShortcutKeys) -> ShortcutKeys {
    text.and_then(|t| ShortcutKeys::parse(t).ok()).unwrap_or(default)
}

fn style_of(file: StyleFile, defaults: &StyleSettings) -> StyleSettings {
    StyleSettings {
        font_family: file.font.unwrap_or_else(|| defaults.font_family.clone()),
        size_pt: file.size_pt.map_or(defaults.size_pt, |n| n.single(defaults.size_pt)),
        bold: file.bold.unwrap_or(defaults.bold),
        italic: file.italic.unwrap_or(defaults.italic),
        text_rgb: colour(file.text.as_deref(), defaults.text_rgb),
        outline_rgb: colour(file.outline.as_deref(), defaults.outline_rgb),
        outline_px: file
            .outline_px
            .map_or(defaults.outline_px, |n| n.single(defaults.outline_px)),
        background_rgb: colour(file.background.as_deref(), defaults.background_rgb),
        background_opacity_percent: file
            .background_opacity_percent
            .map_or(defaults.background_opacity_percent, |n| {
                u8::try_from(n.whole().clamp(0, i64::from(u8::MAX))).unwrap_or_default()
            }),
    }
}

/// A left-out label stays empty so the settings' normalising gives it the line's default for this machine's memory.
fn lines_of(file: &LinesFile, defaults: &[LineSettings]) -> Vec<LineSettings> {
    defaults
        .iter()
        .map(|default| {
            let stored = file.line(default.line);
            LineSettings {
                line: default.line,
                label: stored.label.clone().unwrap_or_default(),
                enabled: stored.enabled.unwrap_or(default.enabled),
                fields: line_fields(default.line)
                    .iter()
                    .copied()
                    .filter(|field| stored.value(*field).unwrap_or_else(|| default.fields.contains(field)))
                    .collect(),
            }
        })
        .collect()
}

fn position_of(file: PositionFile) -> Option<OverlayPosition> {
    Some(OverlayPosition {
        monitor: file.monitor?,
        x: file.x.map_or(0, |n| saturating_i32(n.whole())),
        y: file.y.map_or(0, |n| saturating_i32(n.whole())),
    })
}

/// The seconds as written, in a range `Duration` can hold; the settings' normalising applies the real limits.
fn interval_of(seconds: Number, default: Duration) -> Duration {
    let seconds = seconds.float();
    if seconds.is_nan() {
        default
    } else {
        Duration::from_secs_f64(seconds.clamp(0.0, 1.0e6))
    }
}

impl SettingsFile {
    fn into_settings(self) -> Settings {
        let defaults = Settings::default();
        Settings {
            update_interval: self
                .update_seconds
                .map_or(defaults.update_interval, |n| interval_of(n, defaults.update_interval)),
            show_hide: shortcut(self.show_hide.as_deref(), defaults.show_hide),
            lock_unlock: shortcut(self.lock_unlock.as_deref(), defaults.lock_unlock),
            start_with_windows: self.start_with_windows.unwrap_or(defaults.start_with_windows),
            visible: self.visible.unwrap_or(defaults.visible),
            locked: self.locked.unwrap_or(defaults.locked),
            gpu: self.gpu,
            style: style_of(self.style, &defaults.style),
            lines: lines_of(&self.lines, &defaults.lines),
            position: self.position.and_then(position_of),
        }
    }

    fn of(settings: &Settings) -> Self {
        let style = &settings.style;
        let mut lines = LinesFile::default();
        for stored in &settings.lines {
            let file = lines.line_mut(stored.line);
            file.enabled = Some(stored.enabled);
            file.label = Some(stored.label.clone());
            for field in line_fields(stored.line) {
                *file.slot(*field) = Some(stored.fields.contains(field));
            }
        }
        Self {
            update_seconds: Some(Number::Fractional(settings.update_interval.as_secs_f64())),
            show_hide: Some(settings.show_hide.to_string()),
            lock_unlock: Some(settings.lock_unlock.to_string()),
            start_with_windows: Some(settings.start_with_windows),
            visible: Some(settings.visible),
            locked: Some(settings.locked),
            gpu: settings.gpu.clone(),
            style: StyleFile {
                font: Some(style.font_family.clone()),
                size_pt: Some(Number::Fractional(widen(style.size_pt))),
                bold: Some(style.bold),
                italic: Some(style.italic),
                text: Some(Rgb(style.text_rgb).hex()),
                outline: Some(Rgb(style.outline_rgb).hex()),
                outline_px: Some(Number::Fractional(widen(style.outline_px))),
                background: Some(Rgb(style.background_rgb).hex()),
                background_opacity_percent: Some(Number::Whole(i64::from(style.background_opacity_percent))),
            },
            lines,
            position: settings.position.as_ref().map(|p| PositionFile {
                monitor: Some(p.monitor.clone()),
                x: Some(Number::Whole(i64::from(p.x))),
                y: Some(Number::Whole(i64::from(p.y))),
            }),
        }
    }
}

/// `path` with `suffix` added to its file name.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The settings in one TOML file.
#[derive(Debug)]
pub struct TomlSettingsStore {
    path: PathBuf,
}

impl TomlSettingsStore {
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Sets an unusable file aside so the next save does not destroy what the user wrote; says where it went.
    fn keep_as_bad(&self) -> String {
        let bad = beside(&self.path, ".bad");
        match fs::copy(&self.path, &bad) {
            Ok(_) => bad.display().to_string(),
            Err(error) => {
                tracing::warn!(%error, path = %bad.display(), "the unreadable settings file could not be copied aside");
                String::from("nowhere (the copy failed)")
            }
        }
    }
}

impl SettingsStore for TomlSettingsStore {
    fn load(&self) -> Result<Option<Settings>, LoadSettingsError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(LoadSettingsError::Io(Box::new(error))),
        };
        let parsed = String::from_utf8(bytes)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            .and_then(|text| {
                toml::from_str::<SettingsFile>(&text)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            });
        match parsed {
            Ok(file) => Ok(Some(file.into_settings())),
            Err(source) => Err(LoadSettingsError::Unreadable {
                kept_as: self.keep_as_bad(),
                source,
            }),
        }
    }

    fn save(&self, settings: &Settings) -> Result<(), SaveSettingsError> {
        write_atomically(&self.path, settings).map_err(SaveSettingsError)
    }
}

/// Writes beside the file, flushes it to the disk, and renames it over the old one, so neither a crash mid-write nor a
/// power cut after the rename leaves an empty or half-written settings file (empty text would load as the defaults).
fn write_atomically(path: &Path, settings: &Settings) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let body = toml::to_string(&SettingsFile::of(settings))?;
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = beside(path, ".tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(format!("{HEADER}{body}").as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests;
