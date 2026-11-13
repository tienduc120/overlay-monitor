//! The sensor port every hardware adapter implements, and the [`Sampler`] that turns one round of readings into the
//! overlay's text lines (ADR-0007, ADR-0014).

mod diagnostics;
mod ports;
mod sample;
mod settings;
mod style;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use diagnostics::{GpuFact, SystemFacts, SystemInfo, diagnostics_report};
pub use ports::{
    DriverStatus, Extras, GpuPair, GpuSensors, PawnIoStatus, ReadError, Sensor, Sensors, SettingsStore, StartupTask,
    StartupTaskError,
};
pub use sample::{Outcome, OverlayFrame, ReadingReport, Sampler, SamplerControl, is_news};
pub use settings::{
    Field, InvalidShortcut, Line, LineSettings, LoadSettingsError, OverlayPosition, SaveSettingsError, Settings,
    ShortcutKeys, StyleSettings, blank_frame, line_fields, load_settings, overlay_style,
};
pub use style::OverlayStyle;
