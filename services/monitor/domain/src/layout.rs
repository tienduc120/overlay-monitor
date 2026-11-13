use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use jiff::{SignedDuration, Timestamp};

use crate::reading::{Sample, Snapshot};

pub const SHORTEST_INTERVAL: SignedDuration = SignedDuration::from_millis(250);
pub const LONGEST_INTERVAL: SignedDuration = SignedDuration::from_secs(5);

/// Pulls an update interval into the range the sensors can keep up with.
#[must_use]
pub fn clamp_interval(interval: SignedDuration) -> SignedDuration {
    interval.clamp(SHORTEST_INTERVAL, LONGEST_INTERVAL)
}

/// What the memory is, which names its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryKind {
    Ddr5,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Fps,
    Cpu,
    Gpu,
    Memory,
}

/// One value a line can show.
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

impl LineKind {
    #[must_use]
    pub const fn default_label(self, memory: MemoryKind) -> &'static str {
        match (self, memory) {
            (Self::Fps, _) => "FPS",
            (Self::Cpu, _) => "CPU",
            (Self::Gpu, _) => "GPU",
            (Self::Memory, MemoryKind::Ddr5) => "DDR5",
            (Self::Memory, MemoryKind::Other) => "RAM",
        }
    }

    /// Every value the line can show, in display order.
    #[must_use]
    pub const fn fields(self) -> &'static [Field] {
        match self {
            Self::Fps => &[Field::Fps, Field::FrameTime, Field::WorstFrameTime],
            Self::Cpu => &[Field::Load, Field::Temperature],
            Self::Gpu => &[
                Field::Load,
                Field::Temperature,
                Field::Fan,
                Field::MemoryClock,
                Field::VideoLoad,
            ],
            Self::Memory => &[Field::Load, Field::Used, Field::Temperature, Field::Sticks],
        }
    }

    #[must_use]
    pub const fn default_fields(self) -> &'static [Field] {
        match self {
            Self::Fps => &[Field::Fps],
            Self::Cpu | Self::Gpu | Self::Memory => &[Field::Load, Field::Temperature],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineConfig {
    pub kind: LineKind,
    pub label: String,
    pub enabled: bool,
    /// The ticked values, drawn in `LineKind::fields` order; one the line cannot show is ignored.
    pub fields: Vec<Field>,
}

impl LineConfig {
    /// The values to draw: ticked and shown by this kind of line, in display order.
    fn shown(&self) -> impl Iterator<Item = Field> + '_ {
        self.kind.fields().iter().copied().filter(|f| self.fields.contains(f))
    }

    fn is_drawn(&self) -> bool {
        self.enabled && self.shown().next().is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub lines: Vec<LineConfig>,
    pub interval: SignedDuration,
}

impl Default for Layout {
    fn default() -> Self {
        Self::for_memory(MemoryKind::Ddr5)
    }
}

impl Layout {
    #[must_use]
    pub fn for_memory(memory: MemoryKind) -> Self {
        let line = |kind: LineKind| LineConfig {
            kind,
            label: String::from(kind.default_label(memory)),
            enabled: true,
            fields: kind.default_fields().to_vec(),
        };
        Self {
            lines: alloc::vec![
                line(LineKind::Fps),
                line(LineKind::Cpu),
                line(LineKind::Gpu),
                line(LineKind::Memory),
            ],
            interval: SignedDuration::from_secs(1),
        }
    }

    /// One text line per drawn line config (enabled, with a value ticked), labels padded to the longest drawn label.
    #[must_use]
    pub fn render(&self, snapshot: &Snapshot, now: Timestamp) -> Vec<String> {
        let width = self
            .lines
            .iter()
            .filter(|l| l.is_drawn())
            .map(|l| l.label.chars().count())
            .max()
            .unwrap_or(0)
            + 1;
        let fresh = |sample: Option<Sample>| {
            sample
                .filter(|s| now.duration_since(s.at) <= self.interval.saturating_mul(3))
                .map(|s| s.value)
        };
        self.lines
            .iter()
            .filter(|l| l.is_drawn())
            .map(|l| {
                let label = format!("{:<width$}", format!("{}:", l.label));
                let values: Vec<String> = l
                    .shown()
                    .map(|f| field_text(l.kind, f, snapshot, &fresh, now, self))
                    .collect();
                format!("{label}{}", values.join(", "))
            })
            .collect()
    }
}

/// The slot of the snapshot a line's value comes from.
fn slot(kind: LineKind, field: Field, snapshot: &Snapshot) -> Option<Sample> {
    match (kind, field) {
        (LineKind::Fps, Field::WorstFrameTime) => snapshot.worst_frame_ms,
        (LineKind::Fps, _) => snapshot.fps,
        (LineKind::Cpu, Field::Load) => snapshot.cpu_load,
        (LineKind::Cpu, _) => snapshot.cpu_temp,
        (LineKind::Gpu, Field::Load) => snapshot.gpu_load,
        (LineKind::Gpu, Field::Temperature) => snapshot.gpu_temp,
        (LineKind::Gpu, Field::Fan) => snapshot.gpu_fan_rpm,
        (LineKind::Gpu, Field::MemoryClock) => snapshot.gpu_memory_mhz,
        (LineKind::Gpu, _) => snapshot.gpu_video_load,
        (LineKind::Memory, Field::Load) => snapshot.memory_load,
        (LineKind::Memory, Field::Used) => snapshot.memory_used_gb,
        (LineKind::Memory, _) => snapshot.memory_temp,
    }
}

fn field_text(
    kind: LineKind,
    field: Field,
    snapshot: &Snapshot,
    fresh: &dyn Fn(Option<Sample>) -> Option<f64>,
    now: Timestamp,
    layout: &Layout,
) -> String {
    let value = fresh(slot(kind, field, snapshot));
    match field {
        Field::Fps => format!("{} FPS", number(value, 5, 0)),
        Field::FrameTime => format!(
            "{} ms",
            number(value.filter(|fps| *fps > 0.0).map(|fps| 1000.0 / fps), 5, 1)
        ),
        Field::WorstFrameTime => format!("max {} ms", number(value, 5, 1)),
        Field::Load => format!("{} %", number(value, 5, 1)),
        Field::Temperature => format!("{} °C", number(value, 4, 1)),
        Field::Fan => format!("{} rpm", number(value, 4, 0)),
        Field::MemoryClock => format!("{} MHz", number(value, 4, 0)),
        Field::VideoLoad => format!("video {} %", number(value, 5, 1)),
        Field::Used => format!("{} GB", number(value, 4, 1)),
        Field::Sticks => sticks_text(snapshot, now, layout),
    }
}

fn sticks_text(snapshot: &Snapshot, now: Timestamp, layout: &Layout) -> String {
    let fresh = snapshot
        .memory_sticks
        .as_ref()
        .filter(|s| now.duration_since(s.at) <= layout.interval.saturating_mul(3));
    match fresh {
        Some(sticks) if !sticks.celsius.is_empty() => {
            let each: Vec<String> = sticks.celsius.iter().map(|c| number(Some(*c), 4, 1)).collect();
            format!("{} °C", each.join("/"))
        }
        _ => format!("{} °C", number(None, 4, 1)),
    }
}

/// A right-aligned number, or an em dash in the same width when there is no fresh value.
fn number(value: Option<f64>, width: usize, precision: usize) -> String {
    match value {
        Some(v) => format!("{v:>width$.precision$}"),
        None => format!("{:>width$}", "\u{2014}"),
    }
}

#[cfg(test)]
mod tests;
