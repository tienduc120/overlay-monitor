//! Overlay Monitor. A release build has no console window; a debug build keeps one for the log.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    monitor_api::run()
}
