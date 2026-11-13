//! The monitor's domain: readings, the overlay text they render to, and the style. `no_std` keeps I/O, threads, and
//! the clock out; time arrives as a value (ADR-0006).
#![no_std]

extern crate alloc;

mod layout;
mod reading;
mod shortcut;
mod style;

pub use layout::{
    Field, LONGEST_INTERVAL, Layout, LineConfig, LineKind, MemoryKind, SHORTEST_INTERVAL, clamp_interval,
};
pub use reading::{Sample, Snapshot, Sticks};
pub use shortcut::{Modifiers, Shortcut, ShortcutError, shortcut_text};
pub use style::{Rgb, Style};
