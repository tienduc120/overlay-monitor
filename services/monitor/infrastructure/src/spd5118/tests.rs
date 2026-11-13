use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::time::{Duration, Instant};

use monitor_application::{ReadError, Sensor};
use platform_win32::cpu::CpuVendor;
use platform_win32::mutex::GlobalMutex;

use super::{Ddr5Temperature, HubReader, SmbusRead, celsius, describe_identity, is_hub, smbus_modules};

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

struct FakeBus {
    answers: HashMap<(u8, u64), u64>,
    down: AtomicBool,
    calls: AtomicUsize,
}

impl FakeBus {
    fn with(answers: &[(u8, u64, u64)]) -> Self {
        Self {
            answers: answers.iter().map(|&(a, r, v)| ((a, r), v)).collect(),
            down: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
        }
    }

    fn hub(address: u8, temperature_raw: u64) -> [(u8, u64, u64); 3] {
        [
            (address, 0x00, 0x51),
            (address, 0x01, 0x18),
            (address, 0x31, temperature_raw),
        ]
    }
}

impl SmbusRead for &FakeBus {
    fn read(&self, address: u8, register: u64, _protocol: u64) -> Result<u64, ReadError> {
        self.calls.fetch_add(1, SeqCst);
        if self.down.load(SeqCst) {
            return Err(ReadError::Failed(Box::new(io::Error::other("bus error"))));
        }
        self.answers
            .get(&(address, register))
            .copied()
            .ok_or_else(|| ReadError::Failed(Box::new(io::Error::other("no acknowledge"))))
    }
}

#[test]
fn the_hottest_hub_wins_and_the_hubs_are_kept_after_the_first_scan() {
    let mut answers = FakeBus::hub(0x51, 0x0244).to_vec();
    answers.extend(FakeBus::hub(0x53, 0x0214));
    let bus = FakeBus::with(&answers);
    let mut reader = HubReader::new(
        &bus,
        "Local\\overlay-monitor-test-ddr5-the_hottest_hub_wins_and_the_hubs_are_kept_after_the_first_scan",
    );

    let first = reader.read().unwrap();
    let calls_after_scan = bus.calls.load(SeqCst);
    let second = reader.read().unwrap();

    assert!((first - 36.25).abs() < 1e-9, "{first}");
    assert!((second - 36.25).abs() < 1e-9, "{second}");
    assert_eq!(
        bus.calls.load(SeqCst) - calls_after_scan,
        2,
        "a second read only reads the two known hubs"
    );
}

#[test]
fn a_busy_lock_on_the_first_read_fails_it_without_settling_the_scan() {
    let bus = FakeBus::with(&FakeBus::hub(0x51, 0x0244));
    let mut reader = HubReader::new(
        &bus,
        "Local\\overlay-monitor-test-ddr5-a_busy_lock_on_the_first_read_fails_it_without_settling_the_scan",
    );
    let held = GlobalMutex::acquire(
        "Local\\overlay-monitor-test-ddr5-a_busy_lock_on_the_first_read_fails_it_without_settling_the_scan",
        Duration::from_millis(100),
    )
    .unwrap()
    .unwrap();

    let started = Instant::now();
    let blocked = std::thread::scope(|s| s.spawn(|| reader.read()).join().unwrap());
    let waited = started.elapsed();
    drop(held);
    let retried: Result<f64, ReadError> = reader.read();

    assert!(matches!(blocked, Err(ReadError::Failed(_))), "{blocked:?}");
    assert!(waited < Duration::from_secs(1), "{waited:?}");
    assert!((retried.unwrap() - 36.25).abs() < 1e-9, "the next read scans again");
}

#[test]
fn a_scan_where_nothing_answers_is_a_failure_to_retry_not_an_absence() {
    let bus = FakeBus::with(&FakeBus::hub(0x51, 0x0244));
    bus.down.store(true, SeqCst);
    let mut reader = HubReader::new(
        &bus,
        "Local\\overlay-monitor-test-ddr5-a_scan_where_nothing_answers_is_a_failure_to_retry_not_an_absence",
    );

    let down = reader.read();
    bus.down.store(false, SeqCst);
    let recovered = reader.read();

    assert!(matches!(down, Err(ReadError::Failed(_))), "{down:?}");
    assert!((recovered.unwrap() - 36.25).abs() < 1e-9);
}

#[test]
fn a_completed_scan_with_no_hub_is_unavailable_from_then_on() {
    // Address 0x50 answers, but as an older SPD, so it is not a hub.
    let bus = FakeBus::with(&[(0x50, 0x00, 0x18), (0x50, 0x01, 0x51)]);
    let mut reader = HubReader::new(
        &bus,
        "Local\\overlay-monitor-test-ddr5-a_completed_scan_with_no_hub_is_unavailable_from_then_on",
    );

    let first = reader.read();
    let calls = bus.calls.load(SeqCst);
    let second = reader.read();

    assert!(
        matches!(first, Err(ReadError::Unavailable(ref why)) if why == "no DDR5 SPD hub found"),
        "{first:?}"
    );
    assert!(matches!(second, Err(ReadError::Unavailable(_))), "{second:?}");
    assert_eq!(bus.calls.load(SeqCst), calls, "it does not scan again");
}

#[test]
fn a_failed_hub_read_after_the_scan_is_a_failed_read_that_keeps_the_hubs() {
    let bus = FakeBus::with(&FakeBus::hub(0x51, 0x0244));
    let mut reader = HubReader::new(
        &bus,
        "Local\\overlay-monitor-test-ddr5-a_failed_hub_read_after_the_scan_is_a_failed_read_that_keeps_the_hubs",
    );
    reader.read().unwrap();

    bus.down.store(true, SeqCst);
    let failed = reader.read();
    bus.down.store(false, SeqCst);

    assert!(matches!(failed, Err(ReadError::Failed(_))), "{failed:?}");
    assert!((reader.read().unwrap() - 36.25).abs() < 1e-9);
}

#[test]
fn a_missing_module_file_is_unavailable_and_says_how_to_get_it() {
    let reply = Ddr5Temperature::open(Path::new("no-such-dir"), &CpuVendor::Amd);

    assert!(
        matches!(reply, Err(ReadError::Unavailable(ref why)) if why.contains("cargo xtask modules")),
        "{reply:?}"
    );
}

fn modules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/modules")
}

#[test]
#[ignore = "needs PawnIO, DDR5 on an AMD board, and an elevated terminal"]
fn the_hottest_ddr5_stick_reads_a_plausible_value() {
    let t = Ddr5Temperature::open(&modules(), &CpuVendor::Amd)
        .unwrap()
        .read()
        .unwrap();
    assert!((10.0..=95.0).contains(&t), "{t}");
}

#[test]
fn each_stick_is_reported_in_address_order() {
    let mut answers = FakeBus::hub(0x53, 0x0214).to_vec();
    answers.extend(FakeBus::hub(0x51, 0x0244));
    let bus = FakeBus::with(&answers);
    let mut reader = HubReader::new(
        &bus,
        "Local\\overlay-monitor-test-ddr5-each_stick_is_reported_in_address_order",
    );
    assert_eq!(reader.sticks(), None, "no scan yet");

    reader.read().unwrap();

    assert_eq!(reader.sticks(), Some(vec![36.25, 33.25]));
}

#[test]
#[ignore = "needs PawnIO, DDR5 on an Intel board, and an elevated terminal"]
fn the_intel_board_ddr5_reads_a_plausible_value() {
    let t = Ddr5Temperature::open(&modules(), &CpuVendor::Intel)
        .unwrap()
        .read()
        .unwrap();
    assert!((10.0..=95.0).contains(&t), "{t}");
}

#[test]
fn an_intel_cpu_tries_the_i801_module_first() {
    assert_eq!(smbus_modules(CpuVendor::Intel), ["SmbusI801.bin", "SmbusPIIX4.bin"]);
    assert_eq!(smbus_modules(CpuVendor::Amd), ["SmbusPIIX4.bin", "SmbusI801.bin"]);
    assert_eq!(smbus_modules(CpuVendor::Other), ["SmbusPIIX4.bin", "SmbusI801.bin"]);
}

#[test]
fn the_identity_reads_as_name_io_base_and_pci_ids() {
    let text = describe_identity([0x34_58_49_49_50, 0x0B00, 0x7E12_1462_790B_1022]);

    assert_eq!(text, "PIIX4 at I/O 0x0B00, PCI 1022:790B, subsystem 1462:7E12");
}
