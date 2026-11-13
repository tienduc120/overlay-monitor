use std::sync::{Arc, Mutex, PoisonError};

use monitor_domain::Layout;
use platform_clock::{Clock, Timestamp};

use super::Sampler;
use crate::testing::FakeSensor;
use crate::{ReadError, Sensors};

/// A clock that stands still until the test advances it.
#[derive(Clone)]
struct StepClock(Arc<Mutex<Timestamp>>);

impl StepClock {
    fn at(secs: i64) -> Self {
        Self(Arc::new(Mutex::new(Timestamp::from_second(secs).unwrap())))
    }

    fn advance(&self, secs: i64) {
        let mut now = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *now = Timestamp::from_second(now.as_second() + secs).unwrap();
    }
}

impl Clock for StepClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn sensors(cpu_temp: FakeSensor) -> Sensors {
    Sensors {
        fps: Box::new(FakeSensor::fixed(30.0)),
        cpu_load: Box::new(FakeSensor::fixed(2.0)),
        cpu_temp: Box::new(cpu_temp),
        gpu_load: Box::new(FakeSensor::fixed(4.0)),
        gpu_temp: Box::new(FakeSensor::fixed(56.0)),
        memory_load: Box::new(FakeSensor::fixed(30.9)),
        memory_temp: Box::new(FakeSensor::fixed(35.5)),
    }
}

#[test]
fn every_reading_appears_in_the_frame() {
    let mut s = Sampler::new(sensors(FakeSensor::fixed(45.8)), StepClock::at(0));
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %, 45.8 °C");
}

#[test]
fn an_unavailable_sensor_shows_a_dash_at_once() {
    let cpu_temp = FakeSensor::new(vec![Err(ReadError::Unavailable("no PawnIO".into()))]);
    let mut s = Sampler::new(sensors(cpu_temp), StepClock::at(0));
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %,    — °C");
}

#[test]
fn a_failed_read_keeps_the_last_value_until_it_is_stale() {
    let failed = || Err(ReadError::Failed("SMBus busy".into()));
    let cpu_temp = FakeSensor::new(vec![Ok(50.0), failed()]);
    let clock = StepClock::at(0);
    let mut s = Sampler::new(sensors(cpu_temp), clock.clone());
    s.sample();
    clock.advance(1);
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %, 50.0 °C");
    clock.advance(3);
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %,    — °C");
}

#[test]
fn a_non_finite_reading_is_treated_as_a_failed_read() {
    let cpu_temp = FakeSensor::new(vec![Ok(50.0), Ok(f64::NAN)]);
    let clock = StepClock::at(0);
    let mut s = Sampler::new(sensors(cpu_temp), clock.clone());
    s.sample();
    clock.advance(1);
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %, 50.0 °C");
    clock.advance(3);
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %,    — °C");
}

#[test]
fn the_interval_is_the_layouts() {
    let s = Sampler::new(sensors(FakeSensor::fixed(1.0)), StepClock::at(0));
    assert_eq!(s.interval(), Layout::default().interval.unsigned_abs());
}
