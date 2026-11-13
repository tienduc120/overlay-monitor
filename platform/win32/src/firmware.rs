//! Firmware tables.

use windows::Win32::System::SystemInformation::{FIRMWARE_TABLE_PROVIDER, GetSystemFirmwareTable};

use crate::Win32Error;

/// The raw SMBIOS table (`RawSMBIOSData`): an 8-byte header, then the structures.
pub fn smbios_table() -> Result<Vec<u8>, Win32Error> {
    let provider = FIRMWARE_TABLE_PROVIDER(u32::from_be_bytes(*b"RSMB"));
    // SAFETY: no buffer only asks for the table size.
    let needed = unsafe { GetSystemFirmwareTable(provider, 0, None) };
    if needed == 0 {
        return Err(Win32Error::from_thread());
    }
    let mut table = vec![0_u8; usize::try_from(needed).unwrap_or(0)];
    // SAFETY: the slice is a live, exclusively borrowed buffer that the call fills.
    let written = unsafe { GetSystemFirmwareTable(provider, 0, Some(&mut table)) };
    if written == 0 {
        return Err(Win32Error::from_thread());
    }
    table.truncate(usize::try_from(written).unwrap_or(0));
    Ok(table)
}

#[cfg(test)]
mod tests;
