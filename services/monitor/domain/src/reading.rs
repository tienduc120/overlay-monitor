use jiff::Timestamp;

/// One measured number and when it was taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub value: f64,
    pub at: Timestamp,
}

/// The latest sample of every reading; `None` until a sensor has produced one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub fps: Option<Sample>,
    pub cpu_load: Option<Sample>,
    pub cpu_temp: Option<Sample>,
    pub gpu_load: Option<Sample>,
    pub gpu_temp: Option<Sample>,
    pub memory_load: Option<Sample>,
    pub memory_temp: Option<Sample>,
}
