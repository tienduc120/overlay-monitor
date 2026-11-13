use std::time::Duration;

use super::GlobalMutex;

#[test]
fn a_held_lock_times_out_instead_of_blocking() {
    let name = "Local\\overlay-monitor-test-lock";
    let held = GlobalMutex::acquire(name, Duration::from_millis(100)).unwrap();
    assert!(held.is_some());
    std::thread::scope(|s| {
        let other = s.spawn(|| GlobalMutex::acquire(name, Duration::from_millis(50)).unwrap().is_none());
        assert!(other.join().unwrap(), "a second thread must give up after the timeout");
    });
}
