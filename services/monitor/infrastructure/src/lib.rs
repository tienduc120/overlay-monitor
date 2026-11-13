//! Sensor adapters: the Windows and hardware readings behind the application's [`monitor_application::Sensor`] port.

mod cpu_load;
mod frame_rate;
mod gpu;
mod memory_load;
mod pawn_support;
mod ryzen;
mod settings_file;
mod spd5118;
mod unavailable;

pub use cpu_load::CpuLoad;
pub use frame_rate::{FrameCounter, FrameRate, PRESENT_PROVIDERS, frames_per_second, is_present};
pub use gpu::{GpuLoad, GpuTemperature, engine_load_percent, pick_adapter};
pub use memory_load::MemoryLoad;
pub use ryzen::RyzenTemperature;
pub use settings_file::{TomlSettingsStore, settings_path};
pub use spd5118::Ddr5Temperature;
pub use unavailable::{UnavailableSensor, or_unavailable};
