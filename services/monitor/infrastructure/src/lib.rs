//! Sensor adapters: the Windows and hardware readings behind the application's [`monitor_application::Sensor`] port.

mod cpu_load;
mod frame_rate;
mod gpu;
mod intel;
mod memory_load;
mod memory_type;
mod pawn_support;
mod ryzen;
mod settings_file;
mod spd5118;
mod startup_task;
mod unavailable;

pub use cpu_load::CpuLoad;
pub use frame_rate::{FrameCounter, FrameRate, PRESENT_PROVIDERS, frames_per_second, is_present};
pub use gpu::{AdapterSensors, GpuLoad, GpuTemperature, adapter_for, engine_load_percent, pick_adapter};
pub use intel::{IntelTemperature, package_celsius};
pub use memory_load::MemoryLoad;
pub use memory_type::{memory_kind, memory_text, memory_types};
pub use pawn_support::{OLDEST_PAWNIO, PawnIoProbe, classify, cpu_temperature, pawnio_status, reopening};
pub use ryzen::RyzenTemperature;
pub use settings_file::{TomlSettingsStore, settings_path};
pub use spd5118::Ddr5Temperature;
pub use startup_task::{SchtasksStartup, TASK_NAME, task_xml, utf16_with_bom};
pub use unavailable::{UnavailableSensor, or_unavailable};
