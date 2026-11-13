use std::io;

use monitor_application::{ReadError, Sensor};

use super::{UnavailableSensor, or_unavailable};

struct Fixed(f64);

impl Sensor for Fixed {
    fn read(&mut self) -> Result<f64, ReadError> {
        Ok(self.0)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("load module x.bin")]
struct Outer(#[source] io::Error);

fn reason_of(error: ReadError) -> String {
    match or_unavailable::<Fixed>(Err(error)).read() {
        Err(ReadError::Unavailable(why)) => why,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn an_unavailable_sensor_always_says_why() {
    let mut sensor = UnavailableSensor("no hub".to_owned());

    let first = sensor.read();
    let second = sensor.read();

    assert!(
        matches!(first, Err(ReadError::Unavailable(ref why)) if why == "no hub"),
        "{first:?}"
    );
    assert!(matches!(second, Err(ReadError::Unavailable(_))), "{second:?}");
}

#[test]
fn a_sensor_that_opened_is_kept() {
    let mut kept = or_unavailable(Ok(Fixed(7.0)));

    assert!(matches!(kept.read(), Ok(v) if (v - 7.0).abs() < f64::EPSILON));
}

#[test]
fn an_unavailable_reason_passes_through_unchanged() {
    let reason = reason_of(ReadError::Unavailable("PawnIO needs an elevated process".to_owned()));

    assert_eq!(reason, "PawnIO needs an elevated process");
}

#[test]
fn a_failed_open_keeps_its_whole_cause_chain_in_the_reason() {
    let failure = ReadError::Failed(Box::new(Outer(io::Error::other("disk gone"))));

    let reason = reason_of(failure);

    assert_eq!(reason, "load module x.bin: disk gone");
}
