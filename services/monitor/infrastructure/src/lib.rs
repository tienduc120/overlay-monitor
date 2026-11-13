//! Sensor adapters: the Windows and hardware readings behind the application's [`monitor_application::Sensor`] port.

mod cpu_load;
mod memory_load;

pub use cpu_load::CpuLoad;
pub use memory_load::MemoryLoad;
