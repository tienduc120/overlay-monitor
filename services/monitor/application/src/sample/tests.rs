use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use monitor_domain::Layout;
use platform_clock::{Clock, Timestamp};

use super::{Outcome, Sampler, is_news};
use crate::testing::{FakeGpuSensors, FakeSensor};
use crate::{Extras, Line, ReadError, Sensor, Sensors, Settings, line_fields};

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

fn sampler(sensors: Sensors, clock: StepClock) -> Sampler<StepClock> {
    Sampler::new(
        sensors,
        Box::new(FakeGpuSensors::default()),
        clock,
        &Settings::default(),
    )
}

fn sampler_with_gpus(sensors: Sensors, gpus: &FakeGpuSensors) -> Sampler<StepClock> {
    Sampler::new(sensors, Box::new(gpus.clone()), StepClock::at(0), &Settings::default())
}

fn settings_with_gpu_fields() -> Settings {
    let mut settings = Settings::default();
    for line in &mut settings.lines {
        if line.line == Line::Gpu {
            line.fields = line_fields(Line::Gpu).to_vec();
        }
    }
    settings
}

fn gpu_extras() -> Extras {
    Extras {
        gpu_fan_rpm: Some(1200.0),
        gpu_memory_mhz: Some(644.0),
        ..Extras::default()
    }
}

/// A sensor whose extras differ per answer, which `FakeSensor` does not do.
struct Scripted(VecDeque<(f64, Extras)>, Extras);

impl Sensor for Scripted {
    fn read(&mut self) -> Result<f64, ReadError> {
        if let Some((value, extras)) = self.0.pop_front() {
            self.1 = extras;
            return Ok(value);
        }
        Err(ReadError::Unavailable(String::from("script over")))
    }

    fn extras(&self) -> Extras {
        self.1.clone()
    }
}

#[test]
fn every_reading_appears_in_the_frame() {
    let mut s = sampler(sensors(FakeSensor::fixed(45.8)), StepClock::at(0));
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %, 45.8 °C");
}

#[test]
fn an_unavailable_sensor_shows_a_dash_at_once() {
    let cpu_temp = FakeSensor::new(vec![Err(ReadError::Unavailable("no PawnIO".into()))]);
    let mut s = sampler(sensors(cpu_temp), StepClock::at(0));
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %,    — °C");
}

#[test]
fn a_failed_read_keeps_the_last_value_until_it_is_stale() {
    let failed = || Err(ReadError::Failed("SMBus busy".into()));
    let cpu_temp = FakeSensor::new(vec![Ok(50.0), failed()]);
    let clock = StepClock::at(0);
    let mut s = sampler(sensors(cpu_temp), clock.clone());
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
    let mut s = sampler(sensors(cpu_temp), clock.clone());
    s.sample();
    clock.advance(1);
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %, 50.0 °C");
    clock.advance(3);
    assert_eq!(s.sample().lines[1], "CPU:   2.0 %,    — °C");
}

#[test]
fn the_interval_is_the_layouts() {
    let s = sampler(sensors(FakeSensor::fixed(1.0)), StepClock::at(0));
    assert_eq!(s.interval(), Layout::default().interval.unsigned_abs());
}

#[test]
fn applying_new_lines_and_labels_changes_the_next_frame() {
    let mut s = sampler(sensors(FakeSensor::fixed(45.8)), StepClock::at(0));
    let mut settings = Settings::default();
    for line in &mut settings.lines {
        match line.line {
            Line::Cpu => line.enabled = false,
            Line::Fps => line.label = String::from("Frames"),
            Line::Gpu | Line::Memory => {}
        }
    }
    s.apply(&settings);
    let lines = s.sample().lines;
    assert!(lines[0].starts_with("Frames:"), "{lines:?}");
    assert!(lines.iter().all(|l| !l.starts_with("CPU:")), "{lines:?}");
}

#[test]
fn applying_a_new_interval_changes_the_wait() {
    let mut s = sampler(sensors(FakeSensor::fixed(1.0)), StepClock::at(0));
    s.apply(&Settings {
        update_interval: Duration::from_secs(2),
        ..Settings::default()
    });
    assert_eq!(s.interval(), Duration::from_secs(2));
    s.apply(&Settings {
        update_interval: Duration::from_secs(30),
        ..Settings::default()
    });
    assert_eq!(s.interval(), Duration::from_secs(5));
}

#[test]
fn choosing_another_gpu_opens_it_and_drops_the_old_gpu_values() {
    let gpus = FakeGpuSensors::default();
    let mut s = sampler_with_gpus(sensors(FakeSensor::fixed(1.0)), &gpus);
    assert_eq!(s.sample().lines[2], "GPU:   4.0 %, 56.0 °C");
    s.apply(&Settings {
        gpu: Some(String::from("B")),
        ..Settings::default()
    });
    assert_eq!(gpus.opened(), vec![Some(String::from("B"))]);
    assert_eq!(s.sample().lines[2], "GPU:  11.0 %, 66.0 °C");
}

#[test]
fn keeping_the_same_gpu_leaves_its_sensors_alone() {
    let gpus = FakeGpuSensors::default();
    let mut s = sampler_with_gpus(sensors(FakeSensor::fixed(1.0)), &gpus);
    s.apply(&Settings::default());
    assert_eq!(gpus.opened(), Vec::<Option<String>>::new());
}

#[test]
fn extra_values_come_from_the_sensor_that_owns_them() {
    let mut sensors = sensors(FakeSensor::fixed(1.0));
    sensors.gpu_temp = Box::new(FakeSensor::fixed(56.0).with_extras(gpu_extras()));
    let mut s = sampler(sensors, StepClock::at(0));
    s.apply(&settings_with_gpu_fields());
    let gpu = s.sample().lines[2].clone();
    assert!(gpu.contains("1200 rpm,  644 MHz, video     — %"), "{gpu}");
}

#[test]
fn a_failed_read_keeps_its_extras_until_stale_and_unavailable_clears_them() {
    let failed = || Err(ReadError::Failed("busy".into()));
    let unavailable = || Err(ReadError::Unavailable("gone".into()));
    let mut sensors = sensors(FakeSensor::fixed(1.0));
    sensors.gpu_temp = Box::new(FakeSensor::new(vec![Ok(56.0), failed(), unavailable()]).with_extras(gpu_extras()));
    let clock = StepClock::at(0);
    let mut s = sampler(sensors, clock.clone());
    s.apply(&settings_with_gpu_fields());
    s.sample();
    clock.advance(1);
    let kept = s.sample().lines[2].clone();
    assert!(kept.contains("1200 rpm"), "{kept}");
    let cleared = s.sample().lines[2].clone();
    assert!(cleared.contains("   — rpm,    — MHz"), "{cleared}");
}

#[test]
fn an_extra_the_sensor_does_not_give_shows_a_dash_at_once() {
    let mut sensors = sensors(FakeSensor::fixed(1.0));
    sensors.gpu_temp = Box::new(Scripted(
        VecDeque::from([(56.0, gpu_extras()), (56.0, Extras::default())]),
        Extras::default(),
    ));
    let mut s = sampler(sensors, StepClock::at(0));
    s.apply(&settings_with_gpu_fields());
    assert!(s.sample().lines[2].contains("1200 rpm"));
    let line = s.sample().lines[2].clone();
    assert!(line.contains("   — rpm,    — MHz"), "{line}");
}

#[test]
fn swapping_the_gpu_clears_its_extras() {
    let mut sensors = sensors(FakeSensor::fixed(1.0));
    sensors.gpu_temp = Box::new(FakeSensor::fixed(56.0).with_extras(gpu_extras()));
    let mut s = sampler(sensors, StepClock::at(0));
    let mut settings = settings_with_gpu_fields();
    s.apply(&settings);
    assert!(s.sample().lines[2].contains("1200 rpm"));
    settings.gpu = Some(String::from("B"));
    s.apply(&settings);
    let line = s.sample().lines[2].clone();
    assert!(line.contains("   — rpm,    — MHz"), "{line}");
}

fn outcomes(s: &Sampler<StepClock>) -> Vec<(&'static str, String)> {
    s.report().into_iter().map(|r| (r.reading, r.outcome)).collect()
}

#[derive(Debug, thiserror::Error)]
#[error("lock stayed held")]
struct Busy(#[source] Underlying);

#[derive(Debug, thiserror::Error)]
#[error("by another program")]
struct Underlying;

#[test]
fn the_report_has_every_reading_with_its_value_or_error() {
    let mut sensors = sensors(FakeSensor::new(vec![Err(ReadError::Failed(Box::new(Busy(
        Underlying,
    ))))]));
    sensors.cpu_load = Box::new(FakeSensor::new(vec![Err(ReadError::Unavailable("no counter".into()))]));
    sensors.fps = Box::new(FakeSensor::fixed(f64::NAN));
    let mut s = sampler(sensors, StepClock::at(0));
    s.sample();

    let report = outcomes(&s);

    let names: Vec<_> = report.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "FPS",
            "CPU load",
            "CPU temperature",
            "GPU load",
            "GPU temperature",
            "Memory load",
            "Memory temperature",
            "Worst frame (ms)",
            "GPU fan (rpm)",
            "GPU VRAM clock (MHz)",
            "GPU video load",
            "Memory used (GB)",
            "Memory sticks (°C)",
        ]
    );
    let texts: Vec<_> = report.iter().take(7).map(|(_, text)| text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "failed: the reading was not a finite number",
            "unavailable: no counter",
            "failed: lock stayed held: by another program",
            "4.0",
            "56.0",
            "30.9",
            "35.5",
        ]
    );
}

#[test]
fn a_reading_not_read_yet_says_so() {
    let s = sampler(sensors(FakeSensor::fixed(1.0)), StepClock::at(0));

    let report = outcomes(&s);

    assert!(
        report.iter().take(7).all(|(_, text)| text == "not read yet"),
        "{report:?}"
    );
    assert!(
        report.iter().skip(7).all(|(_, text)| text == "not reported"),
        "{report:?}"
    );
}

#[test]
fn details_come_from_the_sensor() {
    let mut sensors = sensors(FakeSensor::fixed(45.0).with_details("IntelMSR.bin; TjMax 100 °C"));
    sensors.fps = Box::new(FakeSensor::fixed(30.0));
    let s = sampler(sensors, StepClock::at(0));

    let details: Vec<_> = s.report().into_iter().map(|r| r.details).collect();

    assert_eq!(details[2].as_deref(), Some("IntelMSR.bin; TjMax 100 °C"));
    assert_eq!(details[0], None);
    assert!(details.iter().skip(7).all(Option::is_none));
}

#[test]
fn extras_are_reported_even_when_no_line_shows_them() {
    let mut sensors = sensors(FakeSensor::fixed(1.0));
    sensors.gpu_temp = Box::new(FakeSensor::fixed(56.0).with_extras(gpu_extras()));
    let mut s = sampler(sensors, StepClock::at(0));
    s.sample();

    let report = outcomes(&s);

    assert!(
        report.contains(&("GPU fan (rpm)", String::from("1200.0"))),
        "the default layout does not tick the fan: {report:?}"
    );
    assert!(
        report.contains(&("GPU VRAM clock (MHz)", String::from("644.0"))),
        "{report:?}"
    );
    assert!(
        report.contains(&("GPU video load", String::from("not reported"))),
        "{report:?}"
    );
}

#[test]
fn sticks_are_reported_joined_by_slashes() {
    let mut sensors = sensors(FakeSensor::fixed(1.0));
    sensors.memory_temp = Box::new(FakeSensor::fixed(36.3).with_extras(Extras {
        stick_celsius: Some(vec![36.3, 33.3]),
        ..Extras::default()
    }));
    let mut s = sampler(sensors, StepClock::at(0));
    s.sample();

    assert!(outcomes(&s).contains(&("Memory sticks (°C)", String::from("36.3/33.3"))));
}

fn failed(text: &str) -> Outcome {
    Outcome::Failed(text.to_owned())
}

#[test]
fn a_failure_that_repeats_is_news_once() {
    assert!(is_news(&Outcome::Value(1.0), &failed("bus error")));
    assert!(!is_news(&failed("bus error"), &failed("bus error")));
    assert!(is_news(
        &Outcome::NotYet,
        &Outcome::Unavailable(String::from("no driver"))
    ));
    assert!(!is_news(
        &Outcome::Unavailable(String::from("no driver")),
        &Outcome::Unavailable(String::from("no driver"))
    ));
}

#[test]
fn a_new_failure_text_is_news() {
    assert!(is_news(&failed("bus error"), &failed("timeout")));
    assert!(is_news(
        &failed("timeout"),
        &Outcome::Unavailable(String::from("timeout"))
    ));
}

#[test]
fn recovering_is_news_and_a_first_value_is_not() {
    assert!(is_news(&failed("bus error"), &Outcome::Value(40.0)));
    assert!(is_news(
        &Outcome::Unavailable(String::from("no driver")),
        &Outcome::Value(40.0)
    ));
    assert!(!is_news(&Outcome::NotYet, &Outcome::Value(40.0)));
    assert!(!is_news(&Outcome::Value(40.0), &Outcome::Value(41.0)));
}
