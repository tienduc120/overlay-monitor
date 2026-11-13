//! `DDR5` temperature from each module's `SPD5118` hub, over the `SMBus` through `PawnIO`'s `SmbusPIIX4` (AMD) or
//! `SmbusI801` (Intel) module.

use std::path::Path;

use monitor_application::{Extras, ReadError, Sensor};
use platform_win32::cpu::CpuVendor;
use platform_win32::pawnio::PawnModule;

use crate::pawn_support::{call, call_one, load_module, narrow, with_lock};

const AMD_MODULE: &str = "SmbusPIIX4.bin";
const INTEL_MODULE: &str = "SmbusI801.bin";
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

    /// The controller's identity words, for the log; a bus that cannot say reports none.
    fn identity(&self) -> Option<[u64; 3]> {
        None
    }
}

impl SmbusRead for PawnModule {
    fn read(&self, address: u8, register: u64, protocol: u64) -> Result<u64, ReadError> {
        call_one(
            self,
            "ioctl_smbus_xfer",
            &[u64::from(address), READ, register, protocol],
        )
    }

    fn identity(&self) -> Option<[u64; 3]> {
        let words = call(self, "ioctl_identity", &[], 3)
            .inspect_err(|error| tracing::debug!(%error, "the SMBus controller did not give its identity"))
            .ok()?;
        <[u64; 3]>::try_from(words).ok()
    }
}

/// Where the hub scan stands: `None` until a scan completes, then the hubs it found (possibly none).
pub(crate) struct HubReader<B> {
    bus: B,
    lock: &'static str,
    hubs: Option<Vec<u8>>,
    /// The controller, as text, once the first scan has asked it.
    controller: Option<String>,
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
            controller: None,
            last: Vec::new(),
        }
    }

    /// Scans on the first read, under the bus lock. A lock that is busy or a bus that errors fails this read and
    /// leaves the scan for the next one; only a completed scan settles that there is no hub.
    pub(crate) fn read(&mut self) -> Result<f64, ReadError> {
        let Self {
            bus,
            lock,
            hubs,
            controller,
            last,
        } = self;
        with_lock(lock, || {
            if hubs.is_none() {
                *hubs = Some(scan_hubs(bus)?);
                *controller = identity_text(bus);
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
    /// The controller and each hub with its last reading, for the diagnostics report.
    pub(crate) fn details(&self) -> String {
        let hubs = match self.hubs.as_deref() {
            None => return String::from("not scanned yet"),
            Some([]) => String::from("no SPD5118 hub found"),
            Some(found) => {
                let each: Vec<String> = found
                    .iter()
                    .enumerate()
                    .map(|(n, address)| match self.last.get(n) {
                        Some(celsius) => format!("{address:#04X} ({celsius:.1} °C)"),
                        None => format!("{address:#04X}"),
                    })
                    .collect();
                format!("SPD5118 hubs at {}", each.join(", "))
            }
        };
        match &self.controller {
            Some(controller) => format!("{controller}; {hubs}"),
            None => hubs,
        }
    }

    /// Each stick's last reading in address order; `None` until a scan has found hubs.
    pub(crate) fn sticks(&self) -> Option<Vec<f64>> {
        (!self.last.is_empty()).then(|| self.last.clone())
    }
}

fn identity_text<B: SmbusRead>(bus: &B) -> Option<String> {
    let text = describe_identity(bus.identity()?);
    tracing::info!(controller = %text, "DDR5 SMBus controller");
    Some(text)
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
    module: &'static str,
}

impl Ddr5Temperature {
    /// Loads the module; the hubs are found on the first read, so a busy bus now does not become a permanent `—`.
    ///
    /// # Errors
    /// `Unavailable` when the module file, `PawnIO`, elevation, or a supported chipset is missing; `Failed` otherwise.
    pub fn open(modules_dir: &Path, vendor: &CpuVendor) -> Result<Self, ReadError> {
        let (module, file) = load_first(modules_dir, smbus_modules(*vendor))?;
        Ok(Self {
            reader: HubReader::new(module, SMBUS_LOCK),
            module: file,
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

    fn details(&self) -> Option<String> {
        Some(format!("{}; {}", self.module, self.reader.details()))
    }
}

/// The `SMBus` modules in the order to try: the CPU's vendor says which chipset family is likely.
pub(crate) fn smbus_modules(vendor: CpuVendor) -> [&'static str; 2] {
    match vendor {
        CpuVendor::Intel => [INTEL_MODULE, AMD_MODULE],
        CpuVendor::Amd | CpuVendor::Other => [AMD_MODULE, INTEL_MODULE],
    }
}

/// Loads the first module the machine accepts; a module that does not suit this chipset is `Unavailable`, so the next
/// is tried, and when none loads the preferred module's reason is the one reported.
fn load_first(modules_dir: &Path, files: [&'static str; 2]) -> Result<(PawnModule, &'static str), ReadError> {
    let [preferred, fallback] = files;
    match load_module(modules_dir, preferred) {
        Err(ReadError::Unavailable(reason)) => load_module(modules_dir, fallback)
            .map(|module| (module, fallback))
            .map_err(|_second| ReadError::Unavailable(reason)),
        other => other.map(|module| (module, preferred)),
    }
}

/// Reads the controller's identity words as text: its name, I/O base, PCI ids, and subsystem ids.
pub(crate) fn describe_identity(words: [u64; 3]) -> String {
    let [name, io_base, ids] = words;
    let name: String = name
        .to_le_bytes()
        .into_iter()
        .take_while(|&byte| byte != 0)
        .map(char::from)
        .collect();
    let field = |shift: u32| (ids >> shift) & 0xFFFF;
    format!(
        "{name} at I/O {io_base:#06X}, PCI {:04X}:{:04X}, subsystem {:04X}:{:04X}",
        field(0),
        field(16),
        field(32),
        field(48),
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
