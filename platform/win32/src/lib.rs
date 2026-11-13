//! Safe wrappers over Win32 calls. Every `unsafe` block in the workspace lives in this crate, each behind a safe
//! function with a `// SAFETY:` comment.

#![expect(unsafe_code, reason = "this crate wraps Win32 calls in safe functions (ADR-0012)")]

pub mod cpu;
pub mod dpi;
pub mod etw;
pub mod foreground;
pub mod gpu;
pub mod memory;
pub mod mutex;
pub mod overlay;
pub mod pawnio;
pub mod pdh;
#[cfg(feature = "test-support")]
pub mod testing;
pub mod text;
pub mod ui;

pub use windows::core::Error as Win32Error;
