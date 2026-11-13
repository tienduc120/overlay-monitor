use std::path::{Path, PathBuf};

use monitor_application::Sensor;

use super::{Ddr5Temperature, celsius, is_hub};

#[test]
fn a_hub_answers_0x51_then_0x18() {
    assert!(is_hub(0x51, 0x18));
    assert!(!is_hub(0x18, 0x51));
}

#[test]
fn temperature_is_eleven_signed_bits_in_quarter_degrees() {
    assert!((celsius(0x0244) - 36.25).abs() < 1e-9, "stick at 0x51 in the spike");
    assert!((celsius(0x0214) - 33.25).abs() < 1e-9, "stick at 0x53 in the spike");
    assert!((celsius(0x1FF0) + 1.0).abs() < 1e-9, "below zero is two's complement");
}

fn modules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/modules")
}

#[test]
#[ignore = "needs PawnIO, DDR5 on an AMD board, and an elevated terminal"]
fn the_hottest_ddr5_stick_reads_a_plausible_value() {
    let t = Ddr5Temperature::open(&modules()).unwrap().read().unwrap();
    assert!((10.0..=95.0).contains(&t), "{t}");
}
