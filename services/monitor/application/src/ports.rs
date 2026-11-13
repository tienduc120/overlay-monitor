/// Why a sensor could not give a number.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The reading cannot be had on this machine; it shows `—` at once.
    #[error("unavailable: {0}")]
    Unavailable(String),
    /// One read failed; the last value stays until it is stale.
    #[error("read failed")]
    Failed(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// What a sensor's last successful `read` also produced from the same call. `None` means not available now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extras {
    pub worst_frame_ms: Option<f64>,
    pub gpu_fan_rpm: Option<f64>,
    pub gpu_memory_mhz: Option<f64>,
    pub gpu_video_load: Option<f64>,
    pub memory_used_gb: Option<f64>,
    pub stick_celsius: Option<Vec<f64>>,
}

/// One measured quantity. A plain `fn`, not a future: a read blocks briefly on the sampler's own thread (ADR-0014).
pub trait Sensor: Send {
    fn read(&mut self) -> Result<f64, ReadError>;

    /// The extra values the last successful `read` came with; the sampler asks only after an `Ok`.
    fn extras(&self) -> Extras {
        Extras::default()
    }
}

/// The two sensors of one GPU adapter.
pub struct GpuPair {
    pub load: Box<dyn Sensor>,
    pub temp: Box<dyn Sensor>,
}

/// Opens the sensors of a GPU by name, so the sampler can follow the adapter the settings choose.
pub trait GpuSensors: Send {
    /// The adapter named `name`, or the default one when `name` is `None` or unknown.
    fn open(&mut self, name: Option<&str>) -> GpuPair;
}

/// Every reading the overlay can show.
pub struct Sensors {
    pub fps: Box<dyn Sensor>,
    pub cpu_load: Box<dyn Sensor>,
    pub cpu_temp: Box<dyn Sensor>,
    pub gpu_load: Box<dyn Sensor>,
    pub gpu_temp: Box<dyn Sensor>,
    pub memory_load: Box<dyn Sensor>,
    pub memory_temp: Box<dyn Sensor>,
}

/// Whether the app starts when the user signs in to Windows.
pub trait StartupTask {
    /// Registers the start at sign-in when `on`, removes it otherwise. Removing what is not there succeeds.
    ///
    /// # Errors
    /// `StartupTaskError` when Windows refused or the task scheduler could not be run.
    fn set(&self, on: bool) -> Result<(), StartupTaskError>;
}

/// The start at sign-in could not be changed.
#[derive(Debug, thiserror::Error)]
#[error("the start with Windows could not be changed")]
pub struct StartupTaskError(#[source] pub Box<dyn std::error::Error + Send + Sync>);

/// Where the settings live between runs. `Ok(None)` means there is no file yet.
pub trait SettingsStore {
    /// # Errors
    /// `Unreadable` when the file exists but cannot be used, `Io` when it cannot be read.
    fn load(&self) -> Result<Option<crate::Settings>, crate::LoadSettingsError>;

    /// # Errors
    /// `SaveSettingsError` when the settings could not be written.
    fn save(&self, settings: &crate::Settings) -> Result<(), crate::SaveSettingsError>;
}
