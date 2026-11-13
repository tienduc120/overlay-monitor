use std::time::{Duration, Instant};

use super::pump_messages_for;

#[test]
fn pumping_returns_once_its_time_is_up() {
    let start = Instant::now();

    pump_messages_for(Duration::from_millis(100));

    let took = start.elapsed();
    assert!(took >= Duration::from_millis(100), "returned early, after {took:?}");
    assert!(took < Duration::from_secs(2), "overran: {took:?}");
}
