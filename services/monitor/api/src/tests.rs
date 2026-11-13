use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use monitor_application::testing::{FakeGpuSensors, FakeSensor, FakeStartupTask};
use monitor_application::{GpuPair, Line, ReadError, Sampler, Sensor, Sensors, Settings};
use monitor_infrastructure::{FrameCounter, PRESENT_PROVIDERS, UnavailableSensor};
use platform_clock::{FixedClock, Timestamp};
use platform_win32::Win32Error;
use platform_win32::etw::{start_session, stop_session};

use monitor_application::StartupTask;

use super::{
    FrameTrace, SamplerControl, StartupTrim, StopTrace, UnavailableStartup, build_sensors, count_frames_until_stopped,
    is_new_failure, modules_dir, refresh_startup_task, sample_until_stopped, startup_failure_text, wait_to_restart,
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

/// A sampler whose CPU load reads 2.0, then 3.0, so a frame tells whether the hardware was read again.
fn sampler_counting_cpu_reads() -> Sampler<FixedClock> {
    let mut sensors = fixed_sensors();
    sensors.cpu_load = Box::new(FakeSensor::new(vec![Ok(2.0), Ok(3.0)]));
    Sampler::new(
        sensors,
        Box::new(FakeGpuSensors::default()),
        FixedClock(Timestamp::UNIX_EPOCH),
        &Settings::default(),
    )
}

fn without_fps() -> Settings {
    let mut settings = Settings::default();
    for line in &mut settings.lines {
        if line.line == Line::Fps {
            line.enabled = false;
        }
    }
    settings
}

#[test]
fn an_apply_message_redraws_at_once_without_reading_the_hardware() {
    let mut sampler = sampler_counting_cpu_reads();
    let (frames, received) = mpsc::channel();
    let (control, controlled) = mpsc::channel();
    let wakes = Cell::new(0);
    control.send(SamplerControl::Apply(without_fps())).unwrap();
    drop(control);

    sample_until_stopped(&mut sampler, &frames, || wakes.set(wakes.get() + 1), &controlled);

    let all: Vec<_> = received.try_iter().collect();
    assert_eq!(all.len(), 2, "one frame at the start, one after the apply");
    assert!(all[0].lines.iter().any(|l| l.starts_with("FPS:")));
    assert!(all[1].lines.iter().all(|l| !l.starts_with("FPS:")));
    assert_eq!(
        all[1].lines[0], "CPU:   2.0 %, 50.0 °C",
        "the reading the first frame had"
    );
    assert_eq!(wakes.get(), 2);
}

#[test]
fn previews_in_a_row_do_not_put_off_the_next_sample() {
    let mut sampler = sampler_counting_cpu_reads();
    let interval = sampler.interval();
    let (frames, received) = mpsc::channel();
    let (control, controlled) = mpsc::channel();

    let sampled_again = std::thread::scope(|scope| {
        scope.spawn(move || sample_until_stopped(&mut sampler, &frames, || {}, &controlled));
        // A preview every fifth of an interval, as typing into a label sends them, for up to three intervals.
        let sampled_again = (0..15).any(|_| {
            control.send(SamplerControl::Apply(Settings::default())).unwrap();
            pause(interval / 5);
            received
                .try_iter()
                .any(|frame| frame.lines[1].starts_with("CPU:   3.0 %"))
        });
        drop(control);
        sampled_again
    });

    assert!(sampled_again, "the hardware must be read again once an interval passed");
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

#[test]
fn a_report_request_is_answered_without_sampling() {
    let mut sampler = sampler();
    let (frames, received) = mpsc::channel();
    let (control, controlled) = mpsc::channel::<SamplerControl>();
    let (reply, answer) = mpsc::channel();
    control.send(SamplerControl::Report(reply)).unwrap();
    drop(control);

    sample_until_stopped(&mut sampler, &frames, || {}, &controlled);

    let readings = answer.try_recv().expect("the sampler answered");
    assert_eq!(readings.len(), 13, "seven sensors, then six extras");
    assert_eq!(readings[0].reading, "FPS");
    assert_eq!(readings[7].reading, "Worst frame (ms)");
    assert_eq!(
        received.try_iter().count(),
        1,
        "only the first frame: a report does not sample"
    );
}

#[derive(Debug, thiserror::Error)]
#[error("the access was refused")]
struct Refused;

#[derive(Debug, thiserror::Error)]
#[error("create the overlay window")]
struct CreateWindow(#[source] Refused);

#[test]
fn a_startup_failure_names_its_cause_and_the_log() {
    let text = startup_failure_text(&CreateWindow(Refused), Some(Path::new(r"C:\logs")));
    assert_eq!(
        text,
        "Overlay Monitor could not start: create the overlay window: the access was refused\r\n\r\nThe log is in C:\\logs."
    );
}

#[test]
fn without_a_log_folder_the_text_leaves_it_out() {
    let text = startup_failure_text(&CreateWindow(Refused), None);
    assert_eq!(
        text,
        "Overlay Monitor could not start: create the overlay window: the access was refused"
    );
}

/// Counts its stops, and refuses them when asked to, as a session the system will not stop does.
#[derive(Clone, Default)]
struct FakeStopper {
    stops: Arc<AtomicUsize>,
    refuses: bool,
}

impl FakeStopper {
    fn refusing() -> Self {
        Self {
            refuses: true,
            ..Self::default()
        }
    }

    fn stops(&self) -> usize {
        self.stops.load(Ordering::SeqCst)
    }
}

impl StopTrace for FakeStopper {
    fn stop(&self) -> Result<(), Win32Error> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        if self.refuses {
            Err(Win32Error::from_thread())
        } else {
            Ok(())
        }
    }
}

#[test]
fn the_exit_stops_the_trace_that_is_running() {
    let trace = FrameTrace::new();
    let running = FakeStopper::default();
    assert!(trace.begin(running.clone()), "nothing is stopping yet");

    let stopped = trace.stop();
    trace.end();

    assert!(stopped.is_ok(), "{stopped:?}");
    assert_eq!(running.stops(), 1);
    assert!(trace.is_stopping(), "the run ending after the exit does not undo it");
}

#[test]
fn a_trace_started_after_the_exit_is_never_run() {
    let trace = FrameTrace::new();
    trace.stop().unwrap();
    let late = FakeStopper::default();

    assert!(!trace.begin(late.clone()), "its thread must stop it instead");
    assert_eq!(late.stops(), 0, "the exit had nothing to stop");
}

#[test]
fn a_trace_that_already_ended_leaves_nothing_to_stop() {
    let trace = FrameTrace::new();
    let ended = FakeStopper::default();
    trace.begin(ended.clone());
    trace.end();

    trace.stop().unwrap();

    assert_eq!(ended.stops(), 0);
}

#[test]
fn a_refused_stop_is_reported_and_the_trace_stays_stopped() {
    let trace = FrameTrace::new();
    trace.begin(FakeStopper::refusing());

    let stopped = trace.stop();

    assert!(stopped.is_err(), "the caller logs the refusal");
    assert!(!trace.begin(FakeStopper::default()), "no later trace may run");
}

#[test]
fn waiting_to_restart_ends_when_the_app_is_stopping_or_the_sender_is_gone() {
    let (_sender, stop) = mpsc::channel::<()>();
    let trace = FrameTrace::<FakeStopper>::new();
    assert!(wait_to_restart(&stop, &trace, Duration::ZERO));

    trace.stop().unwrap();
    assert!(!wait_to_restart(&stop, &trace, Duration::ZERO));

    let (sender, stop) = mpsc::channel::<()>();
    drop(sender);
    assert!(!wait_to_restart(
        &stop,
        &FrameTrace::<FakeStopper>::new(),
        Duration::from_secs(60)
    ));
}

#[test]
fn a_restart_failure_is_logged_once_until_it_changes() {
    let mut last = None;
    let (denied, busy) = (5, 170);

    let logged = [denied, denied, busy, busy, denied].map(|failure| is_new_failure(&mut last, failure));

    assert_eq!(logged, [true, false, true, false, true]);
}

#[test]
#[ignore = "needs an elevated terminal"]
fn a_trace_stopped_from_outside_is_started_again_and_the_exit_stops_the_new_one() {
    const NAME: &str = "OverlayMonitorRestartTest";
    let first = start_session(NAME, PRESENT_PROVIDERS).unwrap();
    let counter = FrameCounter::new();
    let trace = FrameTrace::new();
    let (sender, stop) = mpsc::channel::<()>();

    let runs = std::thread::scope(|scope| {
        let trace = &trace;
        let counter = &counter;
        let worker = scope.spawn(move || count_frames_until_stopped(first, NAME, counter, trace, &stop));
        pause(Duration::from_millis(500));
        stop_session(NAME).unwrap();
        // The restart waits 5 s; the second session then runs, and only its stopper ends it.
        pause(Duration::from_secs(7));
        trace.stop().unwrap();
        drop(sender);
        worker.join().unwrap()
    });

    assert!(runs >= 2, "the trace ran {runs} time(s)");
}

/// Waits on a channel nobody sends on, which is how a test waits on the clock.
fn pause(time: Duration) {
    let (_keep_open, never) = mpsc::channel::<()>();
    never.recv_timeout(time).ok();
}

#[test]
fn the_start_up_trim_waits_for_its_delay_then_fires_once() {
    let start = Instant::now();
    let mut trim = StartupTrim::new(start, Duration::from_secs(5));

    assert!(!trim.is_due(start));
    assert!(!trim.is_due(start + Duration::from_millis(4_999)));
    assert!(trim.is_due(start + Duration::from_secs(5)));
    assert!(
        !trim.is_due(start + Duration::from_secs(6)),
        "a second trim never fires"
    );
}
