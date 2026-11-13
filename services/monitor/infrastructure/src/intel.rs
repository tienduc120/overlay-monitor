//! Intel package temperature through `PawnIO`'s `IntelMSR` module, read from two model-specific registers.

use std::path::Path;

use monitor_application::{ReadError, Sensor};
use platform_win32::pawnio::PawnModule;

use crate::pawn_support::{call_one, load_module};

const MODULE: &str = "IntelMSR.bin";
/// `IA32_PACKAGE_THERM_STATUS`: bits 22:16 are the degrees below `TjMax`.
const PACKAGE_THERM_STATUS: u64 = 0x1B1;
/// `MSR_TEMPERATURE_TARGET`: bits 23:16 are `TjMax`.
const TEMPERATURE_TARGET: u64 = 0x1A2;

#[derive(Debug, thiserror::Error)]
#[error("the CPU reports a TjMax of 0, so its temperature cannot be derived")]
struct NoTjMax;

/// The package temperature in degrees Celsius.
pub struct IntelTemperature {
    module: PawnModule,
    /// `TjMax` does not change, so it is read once.
    temperature_target: Option<u64>,
}

impl IntelTemperature {
    /// Loads the module from `modules_dir`.
    ///
    /// # Errors
    /// `Unavailable` when the module file, `PawnIO`, elevation, or a supported CPU is missing; `Failed` otherwise.
    pub fn open(modules_dir: &Path) -> Result<Self, ReadError> {
        Ok(Self {
            module: load_module(modules_dir, MODULE)?,
            temperature_target: None,
        })
    }
}

impl std::fmt::Debug for IntelTemperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntelTemperature").finish_non_exhaustive()
    }
}

impl Sensor for IntelTemperature {
    fn read(&mut self) -> Result<f64, ReadError> {
        let target = if let Some(target) = self.temperature_target {
            target
        } else {
            let target = call_one(&self.module, "ioctl_read_msr", &[TEMPERATURE_TARGET])?;
            self.temperature_target = Some(target);
            target
        };
        let status = call_one(&self.module, "ioctl_read_msr", &[PACKAGE_THERM_STATUS])?;
        package_celsius(status, target)
    }
}

/// `TjMax` minus the digital readout; a `TjMax` of 0 means the register is not implemented.
///
/// # Errors
/// `Failed` when `TjMax` is 0.
pub fn package_celsius(therm_status: u64, temperature_target: u64) -> Result<f64, ReadError> {
    let [_, _, readout, ..] = therm_status.to_le_bytes();
    let [_, _, tj_max, ..] = temperature_target.to_le_bytes();
    if tj_max == 0 {
        return Err(ReadError::Failed(Box::new(NoTjMax)));
    }
    let degrees = i32::from(tj_max) - i32::from(readout & 0x7F);
    Ok(f64::from(degrees))
}

#[cfg(test)]
mod tests;
