use std::path::{Path, PathBuf};

use monitor_application::{ReadError, Sensor};
use platform_win32::cpu::cpu_identity;

use super::{RyzenTemperature, tctl_celsius};

#[test]
fn tctl_is_eleven_bits_of_eighths_minus_49_when_the_range_bit_is_set() {
    assert!(
        (tctl_celsius(0x5F3B_0000) - 46.125).abs() < 1e-9,
        "a Zen 4 part read in the spike"
    );
    assert!(
        (tctl_celsius(0x5F30_0000) - 95.125).abs() < 1e-9,
        "range bit 19 clear: no offset"
    );
}

fn modules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/modules")
}

#[test]
#[ignore = "needs PawnIO, an AMD Ryzen CPU, and an elevated terminal"]
fn the_ryzen_temperature_reads_a_plausible_value() {
    let t = RyzenTemperature::open(&modules(), &cpu_identity())
        .unwrap()
        .read()
        .unwrap();
    assert!((15.0..=100.0).contains(&t), "{t}");
}

#[test]
fn a_missing_module_file_is_unavailable_and_says_how_to_get_it() {
    let reply = RyzenTemperature::open(Path::new("no-such-dir"), &cpu_identity());

    assert!(
        matches!(reply, Err(ReadError::Unavailable(ref why)) if why.contains("cargo xtask modules")),
        "{reply:?}"
    );
}

#[test]
fn zen_1_and_zen_plus_models_subtract_their_offset() {
    let offset = |family, brand| super::tctl_offset(family, brand);

    assert!((offset(0x17, "AMD Ryzen 7 1800X Eight-Core Processor") - 20.0).abs() < 1e-9);
    assert!((offset(0x17, "AMD Ryzen 7 2700X Eight-Core Processor") - 10.0).abs() < 1e-9);
    assert!((offset(0x17, "AMD Ryzen Threadripper 1950X 16-Core Processor") - 27.0).abs() < 1e-9);
    assert!(offset(0x17, "AMD Ryzen 7 2700 Eight-Core Processor").abs() < 1e-9);
    assert!(offset(0x19, "AMD Ryzen 7 7700X 8-Core Processor").abs() < 1e-9);
    assert!(offset(0x19, "AMD Ryzen 7 1800X Eight-Core Processor").abs() < 1e-9);
}

#[test]
fn the_details_name_the_module_and_the_offset() {
    assert_eq!(super::details(0.0), "AMDFamily17.bin; Tctl offset 0 °C");
    assert_eq!(super::details(20.0), "AMDFamily17.bin; Tctl offset 20 °C");
}
