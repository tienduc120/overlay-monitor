use std::path::{Path, PathBuf};

use monitor_application::Sensor;

use super::{RyzenTemperature, tctl_celsius};

#[test]
fn tctl_is_eleven_bits_of_eighths_minus_49_when_the_range_bit_is_set() {
    assert!(
        (tctl_celsius(0x5F3B_0000) - 46.125).abs() < 1e-9,
        "the owner's 7800X3D read in the spike"
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
    let t = RyzenTemperature::open(&modules()).unwrap().read().unwrap();
    assert!((15.0..=100.0).contains(&t), "{t}");
}
