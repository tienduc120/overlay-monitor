//! `DDR5` temperature from each module's `SPD5118` hub, over the `SMBus` through `PawnIO`'s `SmbusPIIX4` module.

use std::path::Path;

use monitor_application::{Extras, ReadError, Sensor};
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

/// One `SMBus` read: what the sensor needs from the bus, so its scan and lock logic runs without hardware.
pub(crate) trait SmbusRead {
    fn read(&self, address: u8, register: u64, protocol: u64) -> Result<u64, ReadError>;
}

impl SmbusRead for PawnModule {
    fn read(&self, address: u8, register: u64, protocol: u64) -> Result<u64, ReadError> {
        call_one(
            self,
            "ioctl_smbus_xfer",
            &[u64::from(address), READ, register, protocol],
        )
    }
}

/// Where the hub scan stands: `None` until a scan completes, then the hubs it found (possibly none).
pub(crate) struct HubReader<B> {
    bus: B,
    lock: &'static str,
    hubs: Option<Vec<u8>>,
    /// The last reading of each hub, in address order.
    last: Vec<f64>,
}

#[derive(Debug, thiserror::Error)]
#[error("no SMBus address answered during the hub scan")]
struct ScanError(#[source] ReadError);

impl<B: SmbusRead> HubReader<B> {
    pub(crate) fn new(bus: B, lock: &'static str) -> Self {
        Self {
            bus,
            lock,
            hubs: None,
            last: Vec::new(),
        }
    }

    /// Scans on the first read, under the bus lock. A lock that is busy or a bus that errors fails this read and
    /// leaves the scan for the next one; only a completed scan settles that there is no hub.
    pub(crate) fn read(&mut self) -> Result<f64, ReadError> {
        let Self { bus, lock, hubs, last } = self;
        with_lock(lock, || {
            if hubs.is_none() {
                *hubs = Some(scan_hubs(bus)?);
            }
            let known = hubs.as_deref().unwrap_or_default();
            if known.is_empty() {
                return Err(ReadError::Unavailable("no DDR5 SPD hub found".to_owned()));
            }
            let mut readings = Vec::with_capacity(known.len());
            for &address in known {
                let raw = bus.read(address, REG_TEMPERATURE, PROTOCOL_WORD)?;
                readings.push(celsius(narrow("ioctl_smbus_xfer", raw)?));
            }
            let hottest = readings.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            *last = readings;
            Ok(hottest)
        })
    }
}

impl<B> HubReader<B> {
    /// Each stick's last reading in address order; `None` until a scan has found hubs.
    pub(crate) fn sticks(&self) -> Option<Vec<f64>> {
        (!self.last.is_empty()).then(|| self.last.clone())
    }
}

/// An empty slot does not answer, so one failed address is normal; a scan where nothing answered is not.
fn scan_hubs<B: SmbusRead>(bus: &B) -> Result<Vec<u8>, ReadError> {
    let mut hubs = Vec::new();
    let mut last_error = None;
    let mut answered = false;
    for address in SLOT_ADDRESSES {
        let ids = bus
            .read(address, REG_DEVICE_TYPE, PROTOCOL_BYTE)
            .and_then(|mr0| Ok((mr0, bus.read(address, REG_DEVICE_REVISION, PROTOCOL_BYTE)?)))
            .and_then(|(mr0, mr1)| Ok((narrow("ioctl_smbus_xfer", mr0)?, narrow("ioctl_smbus_xfer", mr1)?)));
        match ids {
            Ok((mr0, mr1)) => {
                answered = true;
                if is_hub(mr0, mr1) {
                    hubs.push(address);
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) if !answered => Err(ReadError::Failed(Box::new(ScanError(error)))),
        Some(_) | None => Ok(hubs),
    }
}

/// The hottest `DDR5` module's temperature in degrees Celsius.
pub struct Ddr5Temperature {
    reader: HubReader<PawnModule>,
}

impl Ddr5Temperature {
    /// Loads the module; the hubs are found on the first read, so a busy bus now does not become a permanent `—`.
    ///
    /// # Errors
    /// `Unavailable` when the module file, `PawnIO`, elevation, or a supported chipset is missing; `Failed` otherwise.
    pub fn open(modules_dir: &Path) -> Result<Self, ReadError> {
        let module = load_module(modules_dir, MODULE)?;
        Ok(Self {
            reader: HubReader::new(module, SMBUS_LOCK),
        })
    }
}

impl std::fmt::Debug for Ddr5Temperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ddr5Temperature")
            .field("hubs", &self.reader.hubs)
            .finish_non_exhaustive()
    }
}

impl Sensor for Ddr5Temperature {
    fn read(&mut self) -> Result<f64, ReadError> {
        self.reader.read()
    }

    fn extras(&self) -> Extras {
        Extras {
            stick_celsius: self.reader.sticks(),
            ..Extras::default()
        }
    }
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
