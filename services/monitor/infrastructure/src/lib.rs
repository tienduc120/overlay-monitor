//! Sensor adapters: the Windows and hardware readings behind the application's [`monitor_application::Sensor`] port.

mod cpu_load;
mod memory_load;
mod pawn_support;
mod ryzen;
mod spd5118;
mod unavailable;

pub use cpu_load::CpuLoad;
pub use memory_load::MemoryLoad;
pub use ryzen::RyzenTemperature;
pub use spd5118::Ddr5Temperature;
pub use unavailable::{UnavailableSensor, or_unavailable};
