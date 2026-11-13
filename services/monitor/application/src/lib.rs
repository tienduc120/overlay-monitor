//! The sensor port every hardware adapter implements, and the [`Sampler`] that turns one round of readings into the
//! overlay's text lines (ADR-0007, ADR-0014).

mod ports;
mod sample;
mod settings;
mod style;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use ports::{
    Extras, GpuPair, GpuSensors, ReadError, Sensor, Sensors, SettingsStore, StartupTask, StartupTaskError,
};
pub use sample::{OverlayFrame, Sampler, SamplerControl};
pub use settings::{
    Field, InvalidShortcut, Line, LineSettings, LoadSettingsError, OverlayPosition, SaveSettingsError, Settings,
    ShortcutKeys, StyleSettings, blank_frame, line_fields, load_settings, overlay_style,
};
pub use style::{OverlayStyle, default_style};
