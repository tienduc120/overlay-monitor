//! The sensor port every hardware adapter implements, and the [`Sampler`] that turns one round of readings into the
//! overlay's text lines (ADR-0007, ADR-0014).

mod ports;
mod sample;
mod style;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use ports::{ReadError, Sensor, Sensors};
pub use sample::{OverlayFrame, Sampler};
pub use style::{OverlayStyle, default_style};
