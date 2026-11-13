use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use monitor_application::testing::FakeSensor;
use monitor_application::{ReadError, Sampler, Sensor, Sensors};
use monitor_infrastructure::FrameCounter;
use platform_clock::{FixedClock, Timestamp};

use super::{build_sensors, modules_dir, sample_until_stopped};

fn fixed_sensors() -> Sensors {
    let fixed = || -> Box<dyn Sensor> { Box::new(FakeSensor::fixed(50.0)) };
    Sensors {
        fps: fixed(),
        cpu_load: fixed(),
        cpu_temp: fixed(),
        gpu_load: fixed(),
        gpu_temp: fixed(),
        memory_load: fixed(),
        memory_temp: fixed(),
    }
}

fn sampler() -> Sampler<FixedClock> {
    Sampler::new(fixed_sensors(), FixedClock(Timestamp::UNIX_EPOCH))
}

#[test]
fn modules_sit_next_to_the_exe() {
    assert_eq!(
        modules_dir(Path::new(r"C:\apps\OverlayMonitor\overlay-monitor.exe")),
        PathBuf::from(r"C:\apps\OverlayMonitor\modules")
    );
}

#[test]
fn without_modules_the_temperatures_are_unavailable_not_errors() {
    let mut s = build_sensors(Path::new(r"C:\does-not-exist"), None, FrameCounter::new());
    assert!(matches!(s.cpu_temp.read(), Err(ReadError::Unavailable(_))));
    assert!(matches!(s.memory_temp.read(), Err(ReadError::Unavailable(_))));
    assert!(
        matches!(s.gpu_temp.read(), Err(ReadError::Unavailable(_))),
        "no adapter chosen"
    );
}

#[test]
fn the_sampler_sends_a_frame_and_wakes_the_ui_before_it_waits_for_a_stop() {
    let mut sampler = sampler();
    let (frames, received) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let wakes = Cell::new(0);
    stop.send(()).unwrap();

    sample_until_stopped(&mut sampler, &frames, || wakes.set(wakes.get() + 1), &stopped);

    assert_eq!(
        received.try_iter().count(),
        1,
        "one frame, sent before the stop was seen"
    );
    assert_eq!(wakes.get(), 1, "the UI must hear of the frame");
}

#[test]
fn the_sampler_stops_once_its_stop_sender_is_gone() {
    let mut sampler = sampler();
    let (frames, received) = mpsc::channel();
    let (stop, stopped) = mpsc::channel::<()>();
    drop(stop);

    sample_until_stopped(&mut sampler, &frames, || {}, &stopped);

    assert_eq!(received.try_iter().count(), 1);
}

#[test]
fn the_sampler_keeps_sampling_once_an_interval_until_stopped() {
    let mut sampler = sampler();
    let interval = sampler.interval();
    let (frames, received) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();

    // The scope joins the sampler thread, so a sampler that never stops hangs this test until nextest fails it.
    std::thread::scope(|scope| {
        scope.spawn(move || sample_until_stopped(&mut sampler, &frames, || {}, &stopped));
        for n in 1..=2 {
            received
                .recv_timeout(interval * 3)
                .unwrap_or_else(|e| panic!("frame {n} must arrive within three intervals: {e}"));
        }
        stop.send(()).unwrap();
    });
}
