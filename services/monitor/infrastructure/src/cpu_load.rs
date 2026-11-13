//! Whole-machine CPU load from the system time counters.

use monitor_application::{ReadError, Sensor};
use platform_win32::Win32Error;
use platform_win32::cpu::{CpuTimes, cpu_times};

#[derive(Debug, thiserror::Error)]
enum CpuLoadError {
    #[error("read cpu times")]
    Times(#[source] Win32Error),
    #[error("the first read only records a baseline")]
    NoBaseline,
    #[error("no cpu time elapsed since the last read")]
    NoElapsedTime,
}

fn failed(error: CpuLoadError) -> ReadError {
    ReadError::Failed(Box::new(error))
}

/// Percent of CPU time spent busy between two reads.
#[derive(Debug, Default)]
pub struct CpuLoad {
    previous: Option<CpuTimes>,
}

impl CpuLoad {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Sensor for CpuLoad {
    fn read(&mut self) -> Result<f64, ReadError> {
        let now = cpu_times().map_err(|e| failed(CpuLoadError::Times(e)))?;
        let previous = self
            .previous
            .replace(now)
            .ok_or_else(|| failed(CpuLoadError::NoBaseline))?;
        load_percent(previous, now).ok_or_else(|| failed(CpuLoadError::NoElapsedTime))
    }
}

/// `kernel` already counts `idle`, so elapsed time is kernel plus user.
fn load_percent(previous: CpuTimes, now: CpuTimes) -> Option<f64> {
    let idle = now.idle.saturating_sub(previous.idle);
    let elapsed = now.kernel.saturating_sub(previous.kernel) + now.user.saturating_sub(previous.user);
    if elapsed == 0 {
        return None;
    }
    #[expect(clippy::cast_precision_loss, reason = "tick deltas stay far below 2^53")]
    let (busy, elapsed) = (elapsed.saturating_sub(idle) as f64, elapsed as f64);
    Some(busy / elapsed * 100.0)
}

#[cfg(test)]
mod tests;
