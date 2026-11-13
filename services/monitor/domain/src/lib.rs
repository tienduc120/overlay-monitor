//! The monitor's domain: readings, the overlay text they render to, and the style. `no_std` keeps I/O, threads, and
//! the clock out; time arrives as a value (ADR-0006).
#![no_std]

extern crate alloc;

mod layout;
mod reading;
mod style;

pub use layout::{Layout, LineConfig, LineKind};
pub use reading::{Sample, Snapshot};
pub use style::{Rgb, Style};
