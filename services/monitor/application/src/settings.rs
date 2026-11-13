use std::error::Error;
use std::fmt;
use std::time::Duration;

use jiff::SignedDuration;
use monitor_domain::{
    Field as DomainField, LONGEST_INTERVAL, Layout, LineConfig, LineKind, MemoryKind, Modifiers, Rgb, Shortcut, Style,
    clamp_interval, shortcut_text,
};

use crate::{OverlayStyle, SettingsStore};

/// Which line of the overlay a setting is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Line {
    Fps,
    Cpu,
    Gpu,
    Memory,
}

/// One value a line can show; a plain mirror of the domain's, for delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Fps,
    FrameTime,
    WorstFrameTime,
    Load,
    Temperature,
    Fan,
    MemoryClock,
    VideoLoad,
    Used,
    Sticks,
}

const LINES: [Line; 4] = [Line::Fps, Line::Cpu, Line::Gpu, Line::Memory];

impl Line {
    const fn kind(self) -> LineKind {
        match self {
            Self::Fps => LineKind::Fps,
            Self::Cpu => LineKind::Cpu,
            Self::Gpu => LineKind::Gpu,
            Self::Memory => LineKind::Memory,
        }
    }

    const fn of(kind: LineKind) -> Self {
        match kind {
            LineKind::Fps => Self::Fps,
            LineKind::Cpu => Self::Cpu,
            LineKind::Gpu => Self::Gpu,
            LineKind::Memory => Self::Memory,
        }
    }
}

impl Field {
    const fn to_domain(self) -> DomainField {
        match self {
            Self::Fps => DomainField::Fps,
            Self::FrameTime => DomainField::FrameTime,
            Self::WorstFrameTime => DomainField::WorstFrameTime,
            Self::Load => DomainField::Load,
            Self::Temperature => DomainField::Temperature,
            Self::Fan => DomainField::Fan,
            Self::MemoryClock => DomainField::MemoryClock,
            Self::VideoLoad => DomainField::VideoLoad,
            Self::Used => DomainField::Used,
            Self::Sticks => DomainField::Sticks,
        }
    }

    const fn from_domain(field: DomainField) -> Self {
        match field {
            DomainField::Fps => Self::Fps,
            DomainField::FrameTime => Self::FrameTime,
            DomainField::WorstFrameTime => Self::WorstFrameTime,
            DomainField::Load => Self::Load,
            DomainField::Temperature => Self::Temperature,
            DomainField::Fan => Self::Fan,
            DomainField::MemoryClock => Self::MemoryClock,
            DomainField::VideoLoad => Self::VideoLoad,
            DomainField::Used => Self::Used,
            DomainField::Sticks => Self::Sticks,
        }
    }
}

/// What a line can show, in display order (the domain's table, as delivery's values).
pub fn line_fields(line: Line) -> &'static [Field] {
    match line {
        Line::Fps => &[Field::Fps, Field::FrameTime, Field::WorstFrameTime],
        Line::Cpu => &[Field::Load, Field::Temperature],
        Line::Gpu => &[
            Field::Load,
            Field::Temperature,
            Field::Fan,
            Field::MemoryClock,
            Field::VideoLoad,
        ],
        Line::Memory => &[Field::Load, Field::Used, Field::Temperature, Field::Sticks],
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineSettings {
    pub line: Line,
    pub label: String,
    pub enabled: bool,
    pub fields: Vec<Field>,
}

/// A global hotkey as plain values: modifiers and a Windows virtual-key code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutKeys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: u8,
}

#[derive(Debug, thiserror::Error)]
#[error("{text} is not a usable shortcut: {reason}")]
pub struct InvalidShortcut {
    pub text: String,
    pub reason: String,
}

impl ShortcutKeys {
    const fn modifiers(self) -> Modifiers {
        Modifiers {
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
        }
    }

    const fn of(shortcut: Shortcut) -> Self {
        let m = shortcut.modifiers();
        Self {
            ctrl: m.ctrl,
            shift: m.shift,
            alt: m.alt,
            key: shortcut.key(),
        }
    }

    /// Reads text like `Ctrl+Shift+O`.
    ///
    /// # Errors
    /// `InvalidShortcut` when the text is not modifiers and one known key, or has neither Ctrl nor Alt.
    pub fn parse(text: &str) -> Result<Self, InvalidShortcut> {
        Shortcut::parse(text).map(Self::of).map_err(|e| InvalidShortcut {
            text: String::from(text),
            reason: e.to_string(),
        })
    }

    pub fn is_valid(self) -> bool {
        Shortcut::new(self.modifiers(), self.key).is_ok()
    }
}

impl fmt::Display for ShortcutKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&shortcut_text(self.modifiers(), self.key))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleSettings {
    pub font_family: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub text_rgb: u32,
    pub outline_rgb: u32,
    pub outline_px: f32,
    pub background_rgb: u32,
    pub background_opacity_percent: u8,
}

impl StyleSettings {
    fn of(style: Style) -> Self {
        Self {
            font_family: style.font_family,
            size_pt: style.size_pt,
            bold: style.bold,
            italic: style.italic,
            text_rgb: style.text.0,
            outline_rgb: style.outline.0,
            outline_px: style.outline_px,
            background_rgb: style.background.0,
            background_opacity_percent: style.background_opacity_percent,
        }
    }

    fn to_domain(&self) -> Style {
        Style {
            font_family: self.font_family.clone(),
            size_pt: self.size_pt,
            bold: self.bold,
            italic: self.italic,
            text: Rgb(self.text_rgb),
            outline: Rgb(self.outline_rgb),
            outline_px: self.outline_px,
            background: Rgb(self.background_rgb),
            background_opacity_percent: self.background_opacity_percent,
        }
    }
}

/// Where the overlay sits: a monitor's name and the offset inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayPosition {
    pub monitor: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub style: StyleSettings,
    pub lines: Vec<LineSettings>,
    pub update_interval: Duration,
    pub gpu: Option<String>,
    pub show_hide: ShortcutKeys,
    pub lock_unlock: ShortcutKeys,
    pub start_with_windows: bool,
    pub visible: bool,
    pub locked: bool,
    pub position: Option<OverlayPosition>,
}

/// An interval as the domain reads it; one too long for a `SignedDuration` is simply the longest allowed.
fn signed(interval: Duration) -> SignedDuration {
    SignedDuration::try_from(interval).unwrap_or(LONGEST_INTERVAL)
}

fn default_shortcut(shortcut: Shortcut, stored: ShortcutKeys) -> ShortcutKeys {
    if stored.is_valid() {
        stored
    } else {
        ShortcutKeys::of(shortcut)
    }
}

impl Settings {
    pub fn defaults(memory: MemoryKind) -> Self {
        let layout = Layout::for_memory(memory);
        Self {
            style: StyleSettings::of(Style::default()),
            lines: layout.lines.iter().map(line_settings).collect(),
            update_interval: layout.interval.unsigned_abs(),
            gpu: None,
            show_hide: ShortcutKeys::of(Shortcut::SHOW_HIDE),
            lock_unlock: ShortcutKeys::of(Shortcut::LOCK_UNLOCK),
            start_with_windows: false,
            visible: true,
            locked: false,
            position: None,
        }
    }

    /// The settings with every value in a range the app can use, for a file edited by hand.
    #[must_use]
    pub fn normalized(self, memory: MemoryKind) -> Self {
        let lines = LINES.map(|line| self.normalized_line(line, memory));
        let mut show_hide = default_shortcut(Shortcut::SHOW_HIDE, self.show_hide);
        let mut lock_unlock = default_shortcut(Shortcut::LOCK_UNLOCK, self.lock_unlock);
        if show_hide == lock_unlock {
            show_hide = ShortcutKeys::of(Shortcut::SHOW_HIDE);
            lock_unlock = ShortcutKeys::of(Shortcut::LOCK_UNLOCK);
        }
        Self {
            style: StyleSettings::of(self.style.to_domain().clamped()),
            lines: lines.into(),
            update_interval: clamp_interval(signed(self.update_interval)).unsigned_abs(),
            gpu: self.gpu.filter(|g| !g.trim().is_empty()),
            show_hide,
            lock_unlock,
            position: self.position.filter(|p| !p.monitor.trim().is_empty()),
            ..self
        }
    }

    fn normalized_line(&self, line: Line, memory: MemoryKind) -> LineSettings {
        let kind = line.kind();
        let stored = self.lines.iter().find(|l| l.line == line);
        let label = stored.map_or("", |l| l.label.trim());
        LineSettings {
            line,
            label: String::from(if label.is_empty() {
                kind.default_label(memory)
            } else {
                label
            }),
            enabled: stored.is_none_or(|l| l.enabled),
            fields: stored.map_or_else(
                || kind.default_fields().iter().copied().map(Field::from_domain).collect(),
                |l| {
                    line_fields(line)
                        .iter()
                        .copied()
                        .filter(|f| l.fields.contains(f))
                        .collect()
                },
            ),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::defaults(MemoryKind::Ddr5)
    }
}

fn line_settings(config: &LineConfig) -> LineSettings {
    LineSettings {
        line: Line::of(config.kind),
        label: config.label.clone(),
        enabled: config.enabled,
        fields: config.fields.iter().copied().map(Field::from_domain).collect(),
    }
}

/// A settings file that could not be used.
#[derive(Debug, thiserror::Error)]
pub enum LoadSettingsError {
    /// The file exists but is not usable; it was set aside under this name so a save does not overwrite it.
    #[error("the settings file is unreadable and was kept as {kept_as}")]
    Unreadable {
        kept_as: String,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("the settings file could not be read")]
    Io(#[source] Box<dyn Error + Send + Sync>),
}

#[derive(Debug, thiserror::Error)]
#[error("the settings could not be saved")]
pub struct SaveSettingsError(#[source] pub Box<dyn Error + Send + Sync>);

/// The stored settings, or the defaults when there are none or they cannot be read (logged, never fatal).
pub fn load_settings(store: &dyn SettingsStore, memory: MemoryKind) -> Settings {
    match store.load() {
        Ok(Some(settings)) => settings.normalized(memory),
        Ok(None) => Settings::defaults(memory),
        Err(e) => {
            let mut chain = e.to_string();
            let mut source = e.source();
            while let Some(cause) = source {
                chain.push_str(": ");
                chain.push_str(&cause.to_string());
                source = cause.source();
            }
            tracing::warn!(error = %chain, "settings not loaded, using the defaults");
            Settings::defaults(memory)
        }
    }
}

/// The overlay's look; an unlocked overlay keeps a visible-to-the-mouse alpha (see `Style::background_alpha`).
pub fn overlay_style(settings: &Settings) -> OverlayStyle {
    let style = &settings.style;
    OverlayStyle {
        font_family: style.font_family.clone(),
        size_pt: style.size_pt,
        bold: style.bold,
        italic: style.italic,
        text_rgb: style.text_rgb,
        outline_rgb: style.outline_rgb,
        outline_px: style.outline_px,
        background_rgb: style.background_rgb,
        background_alpha: style.to_domain().background_alpha(settings.locked),
    }
}

/// The domain's layout for these settings, for the sampler.
pub(crate) fn layout_of(settings: &Settings) -> Layout {
    Layout {
        lines: settings
            .lines
            .iter()
            .map(|l| LineConfig {
                kind: l.line.kind(),
                label: l.label.clone(),
                enabled: l.enabled,
                fields: l.fields.iter().copied().map(Field::to_domain).collect(),
            })
            .collect(),
        interval: signed(settings.update_interval),
    }
}

#[cfg(test)]
mod tests;
