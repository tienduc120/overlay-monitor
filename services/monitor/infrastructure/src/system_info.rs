//! What this machine is, gathered from Windows when the diagnostics report is asked for.

use monitor_application::{GpuFact, SystemFacts, SystemInfo};
use platform_win32::cpu::CpuIdentity;
use platform_win32::gpu::{GpuAdapter, gpu_perf_data};
use platform_win32::registry::{read_string, read_u32};

use crate::{memory_text, pawnio_status};

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
/// The first build of Windows 11, which still carries Windows 10's product name in the registry.
const FIRST_WINDOWS_11_BUILD: u32 = 22000;

/// Gathers the facts live, so a driver installed since the start shows.
#[derive(Debug)]
pub struct WindowsSystemInfo {
    adapters: Vec<GpuAdapter>,
    cpu: CpuIdentity,
    memory_types: Vec<u8>,
}

impl WindowsSystemInfo {
    #[must_use]
    pub fn new(adapters: Vec<GpuAdapter>, cpu: CpuIdentity, memory_types: Vec<u8>) -> Self {
        Self {
            adapters,
            cpu,
            memory_types,
        }
    }
}

impl SystemInfo for WindowsSystemInfo {
    fn facts(&self) -> SystemFacts {
        let memory = memory_text(&self.memory_types);
        SystemFacts {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            windows: windows_version(),
            cpu: cpu_text(&self.cpu),
            memory: if memory.is_empty() {
                String::from("not listed by the firmware")
            } else {
                memory
            },
            pawnio: pawnio_status(),
            gpus: self.adapters.iter().map(gpu_fact).collect(),
        }
    }
}

/// Windows 11 still says "Windows 10" in `ProductName`; the build number tells them apart.
#[must_use]
pub fn windows_name(product: &str, display_version: &str, build: u32, ubr: u32) -> String {
    let product = if build >= FIRST_WINDOWS_11_BUILD {
        product.replacen("Windows 10", "Windows 11", 1)
    } else {
        product.to_owned()
    };
    let version = if display_version.is_empty() {
        String::new()
    } else {
        format!(" {display_version}")
    };
    format!("{product}{version} (build {build}.{ubr})")
}

fn windows_version() -> String {
    let read = || -> Result<String, platform_win32::Win32Error> {
        let product = read_string(CURRENT_VERSION, "ProductName")?;
        // Older releases have no `DisplayVersion`; the name is still worth giving.
        let display = read_string(CURRENT_VERSION, "DisplayVersion").unwrap_or_default();
        let build = read_string(CURRENT_VERSION, "CurrentBuildNumber")?;
        let ubr = read_u32(CURRENT_VERSION, "UBR").unwrap_or(0);
        Ok(windows_name(&product, &display, build.trim().parse().unwrap_or(0), ubr))
    };
    read().unwrap_or_else(|error| format!("unknown ({error})"))
}

fn cpu_text(cpu: &CpuIdentity) -> String {
    format!(
        "{} ({}, family {:#04x}, model {:#04x}, stepping {})",
        cpu.brand, cpu.vendor_id, cpu.family, cpu.model, cpu.stepping
    )
}

fn gpu_fact(adapter: &GpuAdapter) -> GpuFact {
    let temperature = gpu_perf_data(adapter.luid)
        .map_err(|error| error.to_string())
        .and_then(|perf| temperature_fact(perf.temperature_deci_c));
    GpuFact {
        name: adapter.name.clone(),
        dedicated_bytes: adapter.dedicated_bytes,
        temperature,
    }
}

/// A driver that has no temperature leaves 0, which is not a reading.
fn temperature_fact(deci_celsius: u32) -> Result<f64, String> {
    if deci_celsius == 0 {
        return Err(String::from("the driver reports no temperature"));
    }
    Ok(f64::from(deci_celsius) / 10.0)
}

#[cfg(test)]
mod tests;
