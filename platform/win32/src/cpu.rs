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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuVendor {
    Amd,
    Intel,
    Other,
}

/// What CPUID says about the processor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuIdentity {
    pub vendor: CpuVendor,
    pub vendor_id: String,
    pub brand: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
}

#[must_use]
pub fn cpu_identity() -> CpuIdentity {
    let vendor = cpuid(0, 0);
    let vendor_id = text_from_words(&[vendor.ebx, vendor.edx, vendor.ecx]);
    let (family, model, stepping) = decode_signature(cpuid(1, 0).eax);
    let leaves = [0x8000_0002, 0x8000_0003, 0x8000_0004].map(|leaf| {
        let r = cpuid(leaf, 0);
        [r.eax, r.ebx, r.ecx, r.edx]
    });
    let words = <[u32; 12]>::try_from(leaves.concat()).unwrap_or_default();
    CpuIdentity {
        vendor: match vendor_id.as_str() {
            "AuthenticAMD" => CpuVendor::Amd,
            "GenuineIntel" => CpuVendor::Intel,
            _ => CpuVendor::Other,
        },
        vendor_id,
        brand: brand_from_registers(&words),
        family,
        model,
        stepping,
    }
}

/// Splits the CPUID leaf 1 signature into (family, model, stepping), adding the extended fields where the
/// architecture says to.
#[must_use]
pub fn decode_signature(eax: u32) -> (u32, u32, u32) {
    let stepping = eax & 0xF;
    let base_model = (eax >> 4) & 0xF;
    let base_family = (eax >> 8) & 0xF;
    let extended_model = (eax >> 16) & 0xF;
    let extended_family = (eax >> 20) & 0xFF;
    let family = if base_family == 0xF {
        base_family + extended_family
    } else {
        base_family
    };
    let model = if base_family == 0x6 || base_family == 0xF {
        (extended_model << 4) + base_model
    } else {
        base_model
    };
    (family, model, stepping)
}

/// The brand string from CPUID leaves `0x8000_0002..=0x8000_0004`, without its padding.
#[must_use]
pub fn brand_from_registers(words: &[u32; 12]) -> String {
    text_from_words(words)
}

fn text_from_words(words: &[u32]) -> String {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    String::from_utf8_lossy(&bytes)
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_owned()
}

fn cpuid(leaf: u32, sub_leaf: u32) -> core::arch::x86_64::CpuidResult {
    core::arch::x86_64::__cpuid_count(leaf, sub_leaf)
}

#[cfg(test)]
mod tests;
