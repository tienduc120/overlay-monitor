//! GPU load (engine utilization counters) and temperature (D3DKMT) for the followed adapter.

use std::collections::BTreeMap;

use monitor_application::{Extras, GpuPair, GpuSensors, ReadError, Sensor};
use platform_win32::Win32Error;
use platform_win32::gpu::{GpuAdapter, gpu_perf_data};
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
    engine_loads(samples, luid)
        .into_values()
        .reduce(f64::max)
        .map(|busiest| busiest.min(100.0))
}

/// Each lower-cased engine type of one adapter with its load summed over processes.
fn engine_loads(samples: &[(String, f64)], luid: u64) -> BTreeMap<String, f64> {
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
}

/// The fan's rpm. A fan reading 0 before it has ever turned in this run is more likely a driver that does not report the
/// fan than one at rest (a zero-rpm fan idles at 0 too), so it stays `None` until the first nonzero reading; after
/// that every value, 0 included, is real.
#[must_use]
pub(crate) fn fan_value(rpm: u32, seen_spinning: &mut bool) -> Option<f64> {
    *seen_spinning |= rpm > 0;
    (*seen_spinning).then(|| f64::from(rpm))
}

/// The VRAM clock in MHz; a driver that does not report it leaves 0.
#[must_use]
pub(crate) fn memory_mhz(hz: u64) -> Option<f64> {
    #[expect(clippy::cast_precision_loss, reason = "a clock stays far below 2^53 Hz")]
    let mhz = hz as f64 / 1_000_000.0;
    (hz > 0).then_some(mhz)
}

/// The busiest video engine of one adapter (decode, encode, or processing), at most 100. JPEG engines are left out:
/// they are not what a game or a video call uses.
#[must_use]
pub(crate) fn video_load_percent(samples: &[(String, f64)], luid: u64) -> Option<f64> {
    engine_loads(samples, luid)
        .into_iter()
        .filter(|(engine_type, _)| engine_type.contains("video") && !engine_type.contains("jpeg"))
        .map(|(_, load)| load)
        .reduce(f64::max)
        .map(|busiest| busiest.min(100.0))
}

/// Utilization of the followed adapter.
#[derive(Debug)]
pub struct GpuLoad {
    counters: CounterArray,
    luid: u64,
    video: Option<f64>,
}

impl GpuLoad {
    /// # Errors
    /// `Failed` when the engine counters cannot be opened.
    pub fn open(luid: u64) -> Result<Self, ReadError> {
        let counters = CounterArray::open(ENGINE_UTILIZATION).map_err(|e| failed(GpuError::OpenCounters(e)))?;
        Ok(Self {
            counters,
            luid,
            video: None,
        })
    }
}

impl Sensor for GpuLoad {
    fn read(&mut self) -> Result<f64, ReadError> {
        let samples = self.counters.collect().map_err(|e| failed(GpuError::Collect(e)))?;
        if samples.is_empty() {
            return Err(failed(GpuError::NoBaseline));
        }
        self.video = video_load_percent(&samples, self.luid);
        engine_load_percent(&samples, self.luid).ok_or_else(|| failed(GpuError::NoEngines))
    }

    fn extras(&self) -> Extras {
        Extras {
            gpu_video_load: self.video,
            ..Extras::default()
        }
    }
}

/// Temperature of the followed adapter in degrees Celsius.
#[derive(Debug)]
pub struct GpuTemperature {
    luid: u64,
    fan_rpm: Option<f64>,
    memory_mhz: Option<f64>,
    seen_spinning: bool,
}

impl GpuTemperature {
    #[must_use]
    pub fn new(luid: u64) -> Self {
        Self {
            luid,
            fan_rpm: None,
            memory_mhz: None,
            seen_spinning: false,
        }
    }
}

impl Sensor for GpuTemperature {
    fn read(&mut self) -> Result<f64, ReadError> {
        let perf = gpu_perf_data(self.luid).map_err(|e| failed(GpuError::Temperature(e)))?;
        let deci = perf.temperature_deci_c;
        if deci == 0 {
            return Err(ReadError::Unavailable(
                "this GPU driver does not report temperature".to_owned(),
            ));
        }
        self.fan_rpm = fan_value(perf.fan_rpm, &mut self.seen_spinning);
        self.memory_mhz = memory_mhz(perf.memory_hz);
        Ok(f64::from(deci) / 10.0)
    }

    fn extras(&self) -> Extras {
        Extras {
            gpu_fan_rpm: self.fan_rpm,
            gpu_memory_mhz: self.memory_mhz,
            ..Extras::default()
        }
    }
}

#[cfg(test)]
mod tests;
