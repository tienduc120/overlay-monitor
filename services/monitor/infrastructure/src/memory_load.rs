//! Physical memory load.

use monitor_application::{ReadError, Sensor};
use platform_win32::Win32Error;
use platform_win32::memory::{MemoryStatus, memory_status};

#[derive(Debug, thiserror::Error)]
#[error("read memory status")]
struct MemoryStatusError(#[source] Win32Error);

/// Percent of physical memory in use.
#[derive(Debug, Default)]
pub struct MemoryLoad;

impl MemoryLoad {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Sensor for MemoryLoad {
    fn read(&mut self) -> Result<f64, ReadError> {
        let status = memory_status().map_err(|e| ReadError::Failed(Box::new(MemoryStatusError(e))))?;
        load_percent(status).ok_or_else(|| ReadError::Unavailable("the system reports no physical memory".to_owned()))
    }
}

fn load_percent(status: MemoryStatus) -> Option<f64> {
    if status.total_phys == 0 {
        return None;
    }
    let used = status.total_phys.saturating_sub(status.avail_phys);
    #[expect(clippy::cast_precision_loss, reason = "byte counts stay far below 2^53")]
    let (used, total) = (used as f64, status.total_phys as f64);
    Some(used / total * 100.0)
}

#[cfg(test)]
mod tests;
