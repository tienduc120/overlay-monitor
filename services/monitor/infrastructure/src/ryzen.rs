//! Ryzen die temperature (Tctl) through `PawnIO`'s `AMDFamily17` module, read over the SMN.

use std::path::Path;

use monitor_application::{ReadError, Sensor};
use platform_win32::cpu::CpuIdentity;
use platform_win32::pawnio::PawnModule;

use crate::pawn_support::{call_one, load_module, narrow, with_lock};

const MODULE: &str = "AMDFamily17.bin";
const PCI_LOCK: &str = r"Global\Access_PCI";
/// SMN address of the thermal control register.
const THERMAL_CONTROL_TEMPERATURE: u64 = 0x0005_9800;

/// Tctl in degrees Celsius.
pub struct RyzenTemperature {
    module: PawnModule,
    offset: f64,
}

impl RyzenTemperature {
    /// Loads the module from `modules_dir`.
    ///
    /// # Errors
    /// `Unavailable` when the module file, `PawnIO`, elevation, or a supported CPU is missing; `Failed` otherwise.
    pub fn open(modules_dir: &Path, cpu: &CpuIdentity) -> Result<Self, ReadError> {
        Ok(Self {
            module: load_module(modules_dir, MODULE)?,
            offset: tctl_offset(cpu.family, &cpu.brand),
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
        Ok(tctl_celsius(narrow("ioctl_read_smn", raw)?) - self.offset)
    }
}

/// Bits 31:21 count eighths of a degree; with bit 19 set the range is shifted down by 49 degrees.
pub(crate) fn tctl_celsius(raw: u32) -> f64 {
    let eighths = f64::from((raw >> 21) & 0x7FF) / 8.0;
    if raw & (1 << 19) == 0 { eighths } else { eighths - 49.0 }
}

/// Zen 1 and Zen+ parts report Tctl above the die temperature by a fixed amount (Linux `k10temp`'s offset table).
pub(crate) fn tctl_offset(family: u32, brand: &str) -> f64 {
    const OFFSETS: [(&str, f64); 6] = [
        ("AMD Ryzen 5 1600X", 20.0),
        ("AMD Ryzen 7 1700X", 20.0),
        ("AMD Ryzen 7 1800X", 20.0),
        ("AMD Ryzen 7 2700X", 10.0),
        ("AMD Ryzen Threadripper 19", 27.0),
        ("AMD Ryzen Threadripper 29", 27.0),
    ];
    if family != 0x17 {
        return 0.0;
    }
    OFFSETS
        .iter()
        .find(|(name, _)| brand.contains(name))
        .map_or(0.0, |&(_, offset)| offset)
}

#[cfg(test)]
mod tests;
