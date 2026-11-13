//! Physical memory counters.

use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use crate::Win32Error;

/// Physical memory in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStatus {
    pub total_phys: u64,
    pub avail_phys: u64,
}

pub fn memory_status() -> Result<MemoryStatus, Win32Error> {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>().try_into().unwrap_or(u32::MAX),
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: the pointer comes from a live, exclusively borrowed local, and `dwLength` is set as the call requires.
    unsafe { GlobalMemoryStatusEx(&raw mut status)? };
    Ok(MemoryStatus {
        total_phys: status.ullTotalPhys,
        avail_phys: status.ullAvailPhys,
    })
}

#[cfg(test)]
mod tests;
