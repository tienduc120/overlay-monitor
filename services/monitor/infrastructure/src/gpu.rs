//! GPU load (engine utilization counters) and temperature (D3DKMT) for the followed adapter.

use std::collections::BTreeMap;

use monitor_application::{GpuPair, GpuSensors, ReadError, Sensor};
use platform_win32::Win32Error;
use platform_win32::gpu::{GpuAdapter, gpu_temperature_deci_c};
use platform_win32::pdh::CounterArray;

use crate::{UnavailableSensor, or_unavailable};

const ENGINE_UTILIZATION: &str = r"\GPU Engine(*)\Utilization Percentage";

#[derive(Debug, thiserror::Error)]
enum GpuError {
    #[error("open the gpu engine counters")]
    OpenCounters(#[source] Win32Error),
    #[error("collect the gpu engine counters")]
    Collect(#[source] Win32Error),
    #[error("read the gpu temperature")]
    Temperature(#[source] Win32Error),
    #[error("the first read only primes the engine counters")]
    NoBaseline,
    #[error("no engine of this gpu reported utilization")]
    NoEngines,
}

fn failed(error: GpuError) -> ReadError {
    ReadError::Failed(Box::new(error))
}

/// The adapter with the most dedicated memory: the discrete card over an integrated one.
#[must_use]
pub fn pick_adapter(adapters: &[GpuAdapter]) -> Option<&GpuAdapter> {
    adapters.iter().max_by_key(|a| a.dedicated_bytes)
}

/// The adapter named `name` exactly, else the default pick: an unknown name (a card that was removed) still shows a GPU.
#[must_use]
pub fn adapter_for<'a>(adapters: &'a [GpuAdapter], name: Option<&str>) -> Option<&'a GpuAdapter> {
    name.and_then(|name| adapters.iter().find(|a| a.name == name))
        .or_else(|| pick_adapter(adapters))
}

const NO_GPU: &str = "no GPU adapter was found";

/// Opens the sensors of whichever listed adapter the settings choose.
#[derive(Debug)]
pub struct AdapterSensors {
    adapters: Vec<GpuAdapter>,
}

impl AdapterSensors {
    #[must_use]
    pub fn new(adapters: Vec<GpuAdapter>) -> Self {
        Self { adapters }
    }
}

impl GpuSensors for AdapterSensors {
    fn open(&mut self, name: Option<&str>) -> GpuPair {
        let Some(adapter) = adapter_for(&self.adapters, name) else {
            return GpuPair {
                load: Box::new(UnavailableSensor(NO_GPU.to_owned())),
                temp: Box::new(UnavailableSensor(NO_GPU.to_owned())),
            };
        };
        tracing::info!(name = %adapter.name, "following this GPU");
        GpuPair {
            load: or_unavailable(GpuLoad::open(adapter.luid)),
            temp: Box::new(GpuTemperature::new(adapter.luid)),
        }
    }
}

/// The busiest engine type of one adapter, summed over processes, at most 100.
///
/// Instance names look like `pid_1234_luid_0x00000000_0x0000D3E5_phys_0_eng_0_engtype_3D`.
#[must_use]
pub fn engine_load_percent(samples: &[(String, f64)], luid: u64) -> Option<f64> {
    let tag = format!("luid_0x{:08x}_0x{:08x}", luid >> 32, luid & 0xFFFF_FFFF);
    let mut per_type: BTreeMap<String, f64> = BTreeMap::new();
    for (name, value) in samples {
        let name = name.to_lowercase();
        if !name.contains(&tag) {
            continue;
        }
        if let Some((_, engine_type)) = name.rsplit_once("engtype_") {
            *per_type.entry(engine_type.to_owned()).or_default() += value;
        }
    }
    per_type
        .into_values()
        .reduce(f64::max)
        .map(|busiest| busiest.min(100.0))
}

/// Utilization of the followed adapter.
#[derive(Debug)]
pub struct GpuLoad {
    counters: CounterArray,
    luid: u64,
}

impl GpuLoad {
    /// # Errors
    /// `Failed` when the engine counters cannot be opened.
    pub fn open(luid: u64) -> Result<Self, ReadError> {
        let counters = CounterArray::open(ENGINE_UTILIZATION).map_err(|e| failed(GpuError::OpenCounters(e)))?;
        Ok(Self { counters, luid })
    }
}

impl Sensor for GpuLoad {
    fn read(&mut self) -> Result<f64, ReadError> {
        let samples = self.counters.collect().map_err(|e| failed(GpuError::Collect(e)))?;
        if samples.is_empty() {
            return Err(failed(GpuError::NoBaseline));
        }
        engine_load_percent(&samples, self.luid).ok_or_else(|| failed(GpuError::NoEngines))
    }
}

/// Temperature of the followed adapter in degrees Celsius.
#[derive(Debug)]
pub struct GpuTemperature {
    luid: u64,
}

impl GpuTemperature {
    #[must_use]
    pub fn new(luid: u64) -> Self {
        Self { luid }
    }
}

impl Sensor for GpuTemperature {
    fn read(&mut self) -> Result<f64, ReadError> {
        let deci = gpu_temperature_deci_c(self.luid).map_err(|e| failed(GpuError::Temperature(e)))?;
        if deci == 0 {
            return Err(ReadError::Unavailable(
                "this GPU driver does not report temperature".to_owned(),
            ));
        }
        Ok(f64::from(deci) / 10.0)
    }
}

#[cfg(test)]
mod tests;
