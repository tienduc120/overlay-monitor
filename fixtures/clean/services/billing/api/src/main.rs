//! Clean twin: the composition root wires the layers.
use std::process::ExitCode;

fn main() -> ExitCode {
    let wired = billing_infrastructure::load("i-1", 1).is_some() && billing_interfaces::create_status("i-1", 1) == 201;
    if wired { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
