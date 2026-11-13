use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use monitor_application::testing::{FakeGpuSensors, FakeSensor, FakeStartupTask};
use monitor_application::{GpuPair, Line, ReadError, Sampler, Sensor, Sensors, Settings};
use monitor_infrastructure::{FrameCounter, UnavailableSensor};
use platform_clock::{FixedClock, Timestamp};

use monitor_application::StartupTask;

use super::{
    SamplerControl, UnavailableStartup, build_sensors, modules_dir, refresh_startup_task, sample_until_stopped,
};

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
    Sampler::new(
        fixed_sensors(),
        Box::new(FakeGpuSensors::default()),
        FixedClock(Timestamp::UNIX_EPOCH),
        &Settings::default(),
    )
}

fn unavailable_gpu() -> GpuPair {
    let unavailable = || -> Box<dyn Sensor> { Box::new(UnavailableSensor(String::from("no GPU adapter was found"))) };
    GpuPair {
        load: unavailable(),
        temp: unavailable(),
    }
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
    let mut s = build_sensors(Path::new(r"C:\does-not-exist"), unavailable_gpu(), FrameCounter::new());
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
    let (control, controlled) = mpsc::channel::<SamplerControl>();
    let wakes = Cell::new(0);
    drop(control);

    sample_until_stopped(&mut sampler, &frames, || wakes.set(wakes.get() + 1), &controlled);

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
    let (control, controlled) = mpsc::channel::<SamplerControl>();
    drop(control);

    sample_until_stopped(&mut sampler, &frames, || {}, &controlled);

    assert_eq!(received.try_iter().count(), 1);
}

#[test]
fn the_sampler_keeps_sampling_once_an_interval_until_stopped() {
    let mut sampler = sampler();
    let interval = sampler.interval();
    let (frames, received) = mpsc::channel();
    let (control, controlled) = mpsc::channel::<SamplerControl>();

    // The scope joins the sampler thread, so a sampler that never stops hangs this test until nextest fails it.
    std::thread::scope(|scope| {
        scope.spawn(move || sample_until_stopped(&mut sampler, &frames, || {}, &controlled));
        for n in 1..=2 {
            received
                .recv_timeout(interval * 3)
                .unwrap_or_else(|e| panic!("frame {n} must arrive within three intervals: {e}"));
        }
        drop(control);
    });
}

#[test]
fn an_apply_message_samples_again_at_once() {
    let mut sampler = sampler();
    let (frames, received) = mpsc::channel();
    let (control, controlled) = mpsc::channel();
    let wakes = Cell::new(0);
    let mut settings = Settings::default();
    for line in &mut settings.lines {
        if line.line == Line::Fps {
            line.enabled = false;
        }
    }
    control.send(SamplerControl::Apply(settings)).unwrap();
    drop(control);

    sample_until_stopped(&mut sampler, &frames, || wakes.set(wakes.get() + 1), &controlled);

    let all: Vec<_> = received.try_iter().collect();
    assert_eq!(all.len(), 2, "one frame at the start, one after the apply");
    assert!(all[0].lines.iter().any(|l| l.starts_with("FPS:")));
    assert!(all[1].lines.iter().all(|l| !l.starts_with("FPS:")));
    assert_eq!(wakes.get(), 2);
}

#[test]
fn the_startup_task_is_registered_again_at_start_when_the_setting_is_on() {
    let task = FakeStartupTask::default();
    let settings = Settings {
        start_with_windows: true,
        ..Settings::default()
    };

    refresh_startup_task(&settings, &task);

    assert_eq!(task.calls(), [true]);
}

#[test]
fn the_startup_task_is_left_alone_at_start_when_the_setting_is_off() {
    let task = FakeStartupTask::default();

    refresh_startup_task(&Settings::default(), &task);

    assert_eq!(task.calls(), Vec::<bool>::new());
}

#[test]
fn a_refused_startup_task_does_not_stop_the_start() {
    let task = FakeStartupTask::failing();
    let settings = Settings {
        start_with_windows: true,
        ..Settings::default()
    };

    refresh_startup_task(&settings, &task);

    assert_eq!(task.calls(), [true]);
}

#[test]
fn an_unavailable_startup_task_answers_why_to_every_change() {
    let task = UnavailableStartup("the user is unknown".to_owned());

    let refused = [task.set(true), task.set(false)].map(|outcome| outcome.unwrap_err().0.to_string());

    assert_eq!(refused, ["the user is unknown", "the user is unknown"]);
}
