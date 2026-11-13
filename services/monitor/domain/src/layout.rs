use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use jiff::{SignedDuration, Timestamp};

use crate::reading::{Sample, Snapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Fps,
    Cpu,
    Gpu,
    Memory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineConfig {
    pub kind: LineKind,
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub lines: Vec<LineConfig>,
    pub interval: SignedDuration,
}

impl Default for Layout {
    fn default() -> Self {
        let line = |kind, label: &str| LineConfig {
            kind,
            label: String::from(label),
            enabled: true,
        };
        Self {
            lines: alloc::vec![
                line(LineKind::Fps, "FPS"),
                line(LineKind::Cpu, "CPU"),
                line(LineKind::Gpu, "GPU"),
                line(LineKind::Memory, "DDR5"),
            ],
            interval: SignedDuration::from_secs(1),
        }
    }
}

impl Layout {
    /// One text line per enabled line config, labels padded to the longest enabled label.
    pub fn render(&self, snapshot: &Snapshot, now: Timestamp) -> Vec<String> {
        let width = self
            .lines
            .iter()
            .filter(|l| l.enabled)
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
            .filter(|l| l.enabled)
            .map(|l| {
                let label = format!("{:<width$}", format!("{}:", l.label));
                let body = match l.kind {
                    LineKind::Fps => format!("{} FPS", number(fresh(snapshot.fps), 4, 0)),
                    LineKind::Cpu => load_and_temp(fresh(snapshot.cpu_load), fresh(snapshot.cpu_temp)),
                    LineKind::Gpu => load_and_temp(fresh(snapshot.gpu_load), fresh(snapshot.gpu_temp)),
                    LineKind::Memory => load_and_temp(fresh(snapshot.memory_load), fresh(snapshot.memory_temp)),
                };
                format!("{label}{body}")
            })
            .collect()
    }
}

fn load_and_temp(load: Option<f64>, temp: Option<f64>) -> String {
    format!("{} %, {} °C", number(load, 5, 1), number(temp, 4, 1))
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
