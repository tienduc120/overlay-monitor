use super::{GpuFact, SystemFacts, diagnostics_report};
use crate::{PawnIoStatus, ReadingReport};

fn facts() -> SystemFacts {
    SystemFacts {
        app_version: String::from("0.1.0"),
        windows: String::from("Windows 11 Pro 25H2 (build 26300.7171)"),
        cpu: String::from("AMD Ryzen 7 7700X 8-Core Processor (AuthenticAMD, family 0x19, model 0x61, stepping 2)"),
        memory: String::from("DDR5 x2"),
        pawnio: PawnIoStatus::Installed {
            version: String::from("2.2.0"),
        },
        gpus: vec![
            GpuFact {
                name: String::from("AMD Radeon RX 7800 XT"),
                dedicated_bytes: 17_072_000_000,
                temperature: Ok(56.0),
            },
            GpuFact {
                name: String::from("AMD Radeon(TM) Graphics"),
                dedicated_bytes: 536_870_912,
                temperature: Err(String::from("the driver reports no temperature")),
            },
        ],
    }
}

fn reading(name: &'static str, outcome: &str, details: Option<&str>) -> ReadingReport {
    ReadingReport {
        reading: name,
        outcome: outcome.to_owned(),
        details: details.map(str::to_owned),
    }
}

#[test]
fn the_report_lists_the_facts_then_every_reading() {
    let readings = [
        reading("FPS", "30.0", None),
        reading("CPU temperature", "46.1", Some("AMDFamily17.bin; Tctl offset 0 °C")),
        reading(
            "Memory temperature",
            "failed: lock Global\\Access_SMBUS.HTP.Method stayed held by another program",
            Some("SmbusPIIX4.bin; no SPD5118 hub found"),
        ),
    ];

    let report = diagnostics_report(&facts(), &readings);

    let expected = [
        "Overlay Monitor 0.1.0",
        "Windows: Windows 11 Pro 25H2 (build 26300.7171)",
        "CPU: AMD Ryzen 7 7700X 8-Core Processor (AuthenticAMD, family 0x19, model 0x61, stepping 2)",
        "Memory: DDR5 x2",
        "PawnIO: installed, version 2.2.0 ✓",
        "Graphics adapters:",
        "  AMD Radeon RX 7800 XT, 15.9 GiB dedicated, 56.0 °C",
        "  AMD Radeon(TM) Graphics, 0.5 GiB dedicated, temperature: the driver reports no temperature",
        "Readings:",
        "  FPS: 30.0",
        "  CPU temperature: 46.1 [AMDFamily17.bin; Tctl offset 0 °C]",
        "  Memory temperature: failed: lock Global\\Access_SMBUS.HTP.Method stayed held by another program \
         [SmbusPIIX4.bin; no SPD5118 hub found]",
    ];
    assert_eq!(report, expected.join("\r\n") + "\r\n");
}

#[test]
fn a_failed_reading_shows_its_error_and_details() {
    let readings = [reading(
        "GPU load",
        "failed: collect the gpu engine counters",
        Some("adapter A"),
    )];

    let report = diagnostics_report(&facts(), &readings);

    assert!(
        report.ends_with("  GPU load: failed: collect the gpu engine counters [adapter A]\r\n"),
        "{report}"
    );
}

#[test]
fn an_adapter_without_a_temperature_says_why() {
    let report = diagnostics_report(&facts(), &[]);

    assert!(
        report.contains("0.5 GiB dedicated, temperature: the driver reports no temperature\r\n"),
        "{report}"
    );
}
