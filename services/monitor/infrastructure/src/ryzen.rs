//! Ryzen die temperature (Tctl) through `PawnIO`'s `AMDFamily17` module, read over the SMN.

use std::path::Path;

use monitor_application::{ReadError, Sensor};
use platform_win32::pawnio::PawnModule;

use crate::pawn_support::{call_one, load_module, narrow, with_lock};

const MODULE: &str = "AMDFamily17.bin";
const PCI_LOCK: &str = r"Global\Access_PCI";
/// SMN address of the thermal control register.
const THERMAL_CONTROL_TEMPERATURE: u64 = 0x0005_9800;

/// Tctl in degrees Celsius.
pub struct RyzenTemperature {
    module: PawnModule,
}

impl RyzenTemperature {
    /// Loads the module from `modules_dir`.
    ///
    /// # Errors
    /// `Unavailable` when the module file, `PawnIO`, elevation, or a supported CPU is missing; `Failed` otherwise.
    pub fn open(modules_dir: &Path) -> Result<Self, ReadError> {
        Ok(Self {
            module: load_module(modules_dir, MODULE)?,
        })
    }
}

impl std::fmt::Debug for RyzenTemperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RyzenTemperature").finish_non_exhaustive()
    }
}

impl Sensor for RyzenTemperature {
    fn read(&mut self) -> Result<f64, ReadError> {
        let raw = with_lock(PCI_LOCK, || {
            call_one(&self.module, "ioctl_read_smn", &[THERMAL_CONTROL_TEMPERATURE])
        })?;
        Ok(tctl_celsius(narrow("ioctl_read_smn", raw)?))
    }
}

/// Bits 31:21 count eighths of a degree; with bit 19 set the range is shifted down by 49 degrees.
pub(crate) fn tctl_celsius(raw: u32) -> f64 {
    let eighths = f64::from((raw >> 21) & 0x7FF) / 8.0;
    if raw & (1 << 19) == 0 { eighths } else { eighths - 49.0 }
}

#[cfg(test)]
mod tests;
