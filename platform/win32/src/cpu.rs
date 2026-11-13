//! System-wide CPU time counters.

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::GetSystemTimes;

use crate::Win32Error;

/// Cumulative CPU time in 100 ns units. `kernel` includes `idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub kernel: u64,
    pub user: u64,
}

pub fn cpu_times() -> Result<CpuTimes, Win32Error> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: the three pointers come from live, exclusively borrowed locals that outlive the call.
    unsafe { GetSystemTimes(Some(&raw mut idle), Some(&raw mut kernel), Some(&raw mut user))? };
    Ok(CpuTimes {
        idle: combine(idle),
        kernel: combine(kernel),
        user: combine(user),
    })
}

fn combine(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}

#[cfg(test)]
mod tests;
