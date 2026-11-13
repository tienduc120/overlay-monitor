//! `DDR5` temperature from each module's `SPD5118` hub, over the `SMBus` through `PawnIO`'s `SmbusPIIX4` module.

use std::path::Path;

use monitor_application::{ReadError, Sensor};
use platform_win32::pawnio::PawnModule;

use crate::pawn_support::{call_one, load_module, narrow, with_lock};

const MODULE: &str = "SmbusPIIX4.bin";
const SMBUS_LOCK: &str = r"Global\Access_SMBUS.HTP.Method";
/// DIMM slots answer on 0x50 to 0x57.
const SLOT_ADDRESSES: std::ops::RangeInclusive<u8> = 0x50..=0x57;
/// `SMBus` transfer direction: read only; this crate never writes a device.
const READ: u64 = 1;
const PROTOCOL_BYTE: u64 = 2;
const PROTOCOL_WORD: u64 = 3;
const REG_DEVICE_TYPE: u64 = 0x00;
const REG_DEVICE_REVISION: u64 = 0x01;
const REG_TEMPERATURE: u64 = 0x31;

/// The hottest `DDR5` module's temperature in degrees Celsius.
pub struct Ddr5Temperature {
    module: PawnModule,
    hubs: Vec<u8>,
}

impl Ddr5Temperature {
    /// Loads the module and finds every `SPD5118` hub.
    ///
    /// # Errors
    /// `Unavailable` when the module file, `PawnIO`, elevation, a supported chipset, or any hub is missing; `Failed`
    /// otherwise.
    pub fn open(modules_dir: &Path) -> Result<Self, ReadError> {
        let module = load_module(modules_dir, MODULE)?;
        let hubs = with_lock(SMBUS_LOCK, || Ok(scan_hubs(&module)))?;
        if hubs.is_empty() {
            return Err(ReadError::Unavailable("no DDR5 SPD hub found".to_owned()));
        }
        Ok(Self { module, hubs })
    }
}

impl std::fmt::Debug for Ddr5Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ddr5Temperature")
            .field("hubs", &self.hubs)
            .finish_non_exhaustive()
    }
}

impl Sensor for Ddr5Temperature {
    fn read(&mut self) -> Result<f64, ReadError> {
        with_lock(SMBUS_LOCK, || {
            let mut hottest = f64::NEG_INFINITY;
            for &address in &self.hubs {
                let raw = xfer(&self.module, address, REG_TEMPERATURE, PROTOCOL_WORD)?;
                hottest = hottest.max(celsius(narrow("ioctl_smbus_xfer", raw)?));
            }
            Ok(hottest)
        })
    }
}

/// A slot with no module does not answer, which is not an error while scanning.
fn scan_hubs(module: &PawnModule) -> Vec<u8> {
    SLOT_ADDRESSES
        .filter(|&address| {
            let mr0 = xfer(module, address, REG_DEVICE_TYPE, PROTOCOL_BYTE);
            let mr1 = xfer(module, address, REG_DEVICE_REVISION, PROTOCOL_BYTE);
            match (
                mr0.and_then(|v| narrow("ioctl_smbus_xfer", v)),
                mr1.and_then(|v| narrow("ioctl_smbus_xfer", v)),
            ) {
                (Ok(mr0), Ok(mr1)) => is_hub(mr0, mr1),
                _ => false,
            }
        })
        .collect()
}

fn xfer(module: &PawnModule, address: u8, register: u64, protocol: u64) -> Result<u64, ReadError> {
    call_one(
        module,
        "ioctl_smbus_xfer",
        &[u64::from(address), READ, register, protocol],
    )
}

/// An `SPD5118` hub reports device type 0x51 in MR0 and revision 0x18 in MR1.
pub(crate) fn is_hub(mr0: u8, mr1: u8) -> bool {
    mr0 == 0x51 && mr1 == 0x18
}

/// Bits 12:2 are a signed 11-bit count of quarter degrees.
pub(crate) fn celsius(raw: u16) -> f64 {
    let bits = i32::from((raw >> 2) & 0x7FF);
    let signed = if bits & 0x400 == 0 { bits } else { bits - 0x800 };
    f64::from(signed) * 0.25
}

#[cfg(test)]
mod tests;
