use monitor_application::{ReadError, Sensor};

use super::{UnavailableSensor, or_unavailable};

struct Fixed(f64);

impl Sensor for Fixed {
    fn read(&mut self) -> Result<f64, ReadError> {
        Ok(self.0)
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
fn a_failed_open_becomes_an_unavailable_sensor_and_a_good_one_is_kept() {
    let mut kept = or_unavailable(Ok(Fixed(7.0)));
    let mut replaced = or_unavailable::<Fixed>(Err(ReadError::Unavailable("PawnIO is not installed".to_owned())));

    assert!(matches!(kept.read(), Ok(v) if (v - 7.0).abs() < f64::EPSILON));
    let reply = replaced.read();
    assert!(
        matches!(reply, Err(ReadError::Unavailable(ref why)) if why.contains("PawnIO is not installed")),
        "{reply:?}"
    );
}
