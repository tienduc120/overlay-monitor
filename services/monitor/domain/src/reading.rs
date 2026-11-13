use alloc::vec::Vec;

use jiff::Timestamp;

/// One measured number and when it was taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub value: f64,
    pub at: Timestamp,
}

/// The temperature of each memory stick, in address order, read together.
#[derive(Clone, Debug, PartialEq)]
pub struct Sticks {
    pub celsius: Vec<f64>,
    pub at: Timestamp,
}

/// The latest sample of every reading; `None` until a sensor has produced one. Frame time is `1000 / fps`, so it has
/// no slot of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub fps: Option<Sample>,
    pub worst_frame_ms: Option<Sample>,
    pub cpu_load: Option<Sample>,
    pub cpu_temp: Option<Sample>,
    pub gpu_load: Option<Sample>,
    pub gpu_temp: Option<Sample>,
    pub gpu_fan_rpm: Option<Sample>,
    pub gpu_memory_mhz: Option<Sample>,
    pub gpu_video_load: Option<Sample>,
    pub memory_load: Option<Sample>,
    pub memory_used_gb: Option<Sample>,
    pub memory_temp: Option<Sample>,
    pub memory_sticks: Option<Sticks>,
}
