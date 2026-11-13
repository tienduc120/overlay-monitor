//! Safe wrappers over Win32 calls. Every `unsafe` block in the workspace lives in this crate, each behind a safe
//! function with a `// SAFETY:` comment.

#![expect(unsafe_code, reason = "this crate wraps Win32 calls in safe functions (ADR-0012)")]

pub mod cpu;
pub mod dialog;
pub mod dpi;
pub mod etw;
pub mod foreground;
pub mod gpu;
pub mod hotkey;
pub mod icon;
pub mod instance;
pub mod known_folder;
pub mod memory;
pub mod menu;
pub mod mutex;
pub mod overlay;
pub mod pawnio;
pub mod pdh;
pub mod placement;
pub mod snap;
#[cfg(feature = "test-support")]
pub mod testing;
pub mod text;
pub mod tray;
pub mod ui;

pub use windows::core::Error as Win32Error;
