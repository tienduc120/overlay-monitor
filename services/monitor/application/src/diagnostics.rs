//! The plain-text report a user pastes into a bug report: what this machine is and what each reading says.

use std::fmt::Write;

use crate::{PawnIoStatus, ReadingReport};

const MIB_PER_GIB: f64 = 1024.0;

/// What one graphics adapter is and whether its temperature can be read.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuFact {
    pub name: String,
    pub dedicated_bytes: u64,
    /// The temperature in degrees Celsius, or why the driver gave none.
    pub temperature: Result<f64, String>,
}

/// What this machine is, gathered when the report is asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemFacts {
    pub app_version: String,
    pub windows: String,
    pub cpu: String,
    pub memory: String,
    pub pawnio: PawnIoStatus,
    pub gpus: Vec<GpuFact>,
}

/// Where the facts come from, so the report does not depend on Windows.
pub trait SystemInfo {
    fn facts(&self) -> SystemFacts;
}

/// The report, with CRLF line ends so it pastes cleanly anywhere on Windows.
#[must_use]
pub fn diagnostics_report(facts: &SystemFacts, readings: &[ReadingReport]) -> String {
    let mut lines = vec![
        format!("Overlay Monitor {}", facts.app_version),
        format!("Windows: {}", facts.windows),
        format!("CPU: {}", facts.cpu),
        format!("Memory: {}", facts.memory),
        format!("PawnIO: {}", facts.pawnio.describe()),
        String::from("Graphics adapters:"),
    ];
    lines.extend(facts.gpus.iter().map(gpu_line));
    lines.push(String::from("Readings:"));
    lines.extend(readings.iter().map(reading_line));
    let mut report = String::new();
    for line in lines {
        report.push_str(&line);
        report.push_str("\r\n");
    }
    report
}

fn gpu_line(gpu: &GpuFact) -> String {
    let mut line = format!("  {}, {:.1} GiB dedicated, ", gpu.name, gib(gpu.dedicated_bytes));
    match &gpu.temperature {
        Ok(celsius) => write!(line, "{celsius:.1} °C"),
        Err(reason) => write!(line, "temperature: {reason}"),
    }
    .ok();
    line
}

fn reading_line(reading: &ReadingReport) -> String {
    let mut line = format!("  {}: {}", reading.reading, reading.outcome);
    if let Some(details) = &reading.details {
        write!(line, " [{details}]").ok();
    }
    line
}

/// Bytes as GiB, counted in whole MiB: a `u32` converts to `f64` exactly, and the report shows a tenth of a GiB.
fn gib(bytes: u64) -> f64 {
    let mib = u32::try_from(bytes >> 20).unwrap_or(u32::MAX);
    f64::from(mib) / MIB_PER_GIB
}

#[cfg(test)]
mod tests;
