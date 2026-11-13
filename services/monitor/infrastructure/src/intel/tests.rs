use std::path::{Path, PathBuf};

use monitor_application::{ReadError, Sensor};

use super::{IntelTemperature, package_celsius};

#[test]
fn the_package_temperature_is_tjmax_minus_the_readout() {
    let t = package_celsius(0x8835_0000, 0x0064_0000).unwrap();

    assert!((t - 47.0).abs() < 1e-9, "{t}");
}

#[test]
fn a_zero_tjmax_is_a_failed_read() {
    let reply = package_celsius(0x8835_0000, 0);

    assert!(matches!(reply, Err(ReadError::Failed(_))), "{reply:?}");
}

#[test]
fn a_missing_module_file_is_unavailable_and_says_how_to_get_it() {
    let reply = IntelTemperature::open(Path::new("no-such-dir"));

    assert!(
        matches!(reply, Err(ReadError::Unavailable(ref why)) if why.contains("cargo xtask modules")),
        "{reply:?}"
    );
}

fn modules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/modules")
}

#[test]
#[ignore = "needs PawnIO, an Intel CPU, and an elevated terminal"]
fn the_intel_package_temperature_reads_a_plausible_value() {
    let t = IntelTemperature::open(&modules()).unwrap().read().unwrap();
    assert!((15.0..=105.0).contains(&t), "{t}");
}
