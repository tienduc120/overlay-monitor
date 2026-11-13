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

/// One measured quantity. A plain `fn`, not a future: a read blocks briefly on the sampler's own thread (ADR-0014).
pub trait Sensor: Send {
    fn read(&mut self) -> Result<f64, ReadError>;
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

/// Where the settings live between runs. `Ok(None)` means there is no file yet.
pub trait SettingsStore {
    /// # Errors
    /// `Unreadable` when the file exists but cannot be used, `Io` when it cannot be read.
    fn load(&self) -> Result<Option<crate::Settings>, crate::LoadSettingsError>;

    /// # Errors
    /// `SaveSettingsError` when the settings could not be written.
    fn save(&self, settings: &crate::Settings) -> Result<(), crate::SaveSettingsError>;
}
