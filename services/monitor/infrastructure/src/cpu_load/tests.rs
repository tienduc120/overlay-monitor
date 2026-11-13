use monitor_application::{ReadError, Sensor};
use platform_win32::cpu::CpuTimes;

use super::{CpuLoad, load_percent};

#[test]
fn load_is_busy_time_over_elapsed_time() {
    let prev = CpuTimes {
        idle: 100,
        kernel: 200,
        user: 50,
    };
    let now = CpuTimes {
        idle: 150,
        kernel: 300,
        user: 100,
    };
    let load = load_percent(prev, now).unwrap();
    assert!((load - 66.667).abs() < 0.01, "busy 100 of 150: {load}");
}

#[test]
fn no_elapsed_time_gives_no_load() {
    let t = CpuTimes {
        idle: 1,
        kernel: 2,
        user: 3,
    };
    assert_eq!(load_percent(t, t), None);
}

#[test]
fn the_first_read_has_nothing_to_compare_with() {
    assert!(matches!(CpuLoad::new().read(), Err(ReadError::Failed(_))));
}
