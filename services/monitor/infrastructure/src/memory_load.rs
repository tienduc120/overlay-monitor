//! Physical memory load.

use monitor_application::{Extras, ReadError, Sensor};
use platform_win32::Win32Error;
use platform_win32::memory::{MemoryStatus, memory_status};

#[derive(Debug, thiserror::Error)]
#[error("read memory status")]
struct MemoryStatusError(#[source] Win32Error);

/// Percent of physical memory in use.
#[derive(Debug, Default)]
pub struct MemoryLoad {
    used_gb: Option<f64>,
}

impl MemoryLoad {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Sensor for MemoryLoad {
    fn read(&mut self) -> Result<f64, ReadError> {
        let status = memory_status().map_err(|e| ReadError::Failed(Box::new(MemoryStatusError(e))))?;
        self.used_gb = Some(used_gb(status));
        load_percent(status).ok_or_else(|| ReadError::Unavailable("the system reports no physical memory".to_owned()))
    }

    fn extras(&self) -> Extras {
        Extras {
            memory_used_gb: self.used_gb,
            ..Extras::default()
        }
    }
}

/// Windows' "GB" in Task Manager: gibibytes, here counted in MiB.
const MIB_PER_GB: f64 = 1024.0;

/// Counted in whole MiB: a `u32` converts to `f64` exactly, and the overlay shows a tenth of a GB.
fn used_gb(status: MemoryStatus) -> f64 {
    let used = status.total_phys.saturating_sub(status.avail_phys);
    f64::from(u32::try_from(used >> 20).unwrap_or(u32::MAX)) / MIB_PER_GB
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
