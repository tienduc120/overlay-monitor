//! Safe wrappers over Win32 calls. Every `unsafe` block in the workspace lives in this crate, each behind a safe
//! function with a `// SAFETY:` comment.

#![expect(unsafe_code, reason = "this crate wraps Win32 calls in safe functions (ADR-0012)")]

pub mod cpu;
pub mod memory;

pub use windows::core::Error as Win32Error;
