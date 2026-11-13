//! Physical memory counters.

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows::core::HSTRING;

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

/// This process's memory, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessMemory {
    pub working_set: u64,
    pub peak_working_set: u64,
    /// Memory only this process holds, which Task Manager's Memory column shows (about).
    pub private_bytes: u64,
}

/// What this process holds now.
///
/// # Errors
/// The system would not report it.
pub fn process_memory() -> Result<ProcessMemory, Win32Error> {
    let size = size_of::<PROCESS_MEMORY_COUNTERS_EX>().try_into().unwrap_or(u32::MAX);
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: size,
        ..PROCESS_MEMORY_COUNTERS_EX::default()
    };
    // SAFETY: the pseudo handle of this process is always valid; the buffer is a live local whose size is passed, and
    // the extended counters begin with the plain ones, which is the layout the call documents.
    unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            size,
        )
    }?;
    let wide = |bytes: usize| u64::try_from(bytes).unwrap_or(u64::MAX);
    Ok(ProcessMemory {
        working_set: wide(counters.WorkingSetSize),
        peak_working_set: wide(counters.PeakWorkingSetSize),
        private_bytes: wide(counters.PrivateUsage),
    })
}

/// Whether a DLL of this name is loaded into the process now.
pub fn is_module_loaded(name: &str) -> bool {
    // SAFETY: the name is a live, NUL-terminated string; the call neither loads the module nor takes a reference.
    unsafe { GetModuleHandleW(&HSTRING::from(name)) }.is_ok()
}

/// Gives the pages this process is not using back to the system, once. Windows takes back what is idle and the process
/// faults in again what it still needs, so call it after a burst of allocation ends, never on a timer.
///
/// # Errors
/// The system refused to trim.
pub fn trim_working_set() -> Result<(), Win32Error> {
    // SAFETY: the pseudo handle of this process is always valid; `usize::MAX` for both sizes is the documented request
    // to empty the working set.
    unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) }
}

#[cfg(test)]
mod tests;
