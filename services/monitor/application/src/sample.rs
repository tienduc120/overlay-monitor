use std::error::Error;
use std::sync::mpsc::Sender;
use std::time::Duration;

use monitor_domain::{Layout, Sample, Snapshot, Sticks, clamp_interval};
use platform_clock::{Clock, Timestamp};

use crate::settings::layout_of;
use crate::{Extras, GpuSensors, ReadError, Sensor, Sensors, Settings};

/// What the UI thread tells the sampler; it lives here, not in the root, because the delivery layer holds the sender.
#[derive(Debug)]
pub enum SamplerControl {
    /// Follow these settings, and sample again at once.
    Apply(Settings),
    /// Answer with every reading as it stands, without sampling again.
    Report(Sender<Vec<ReadingReport>>),
}

/// What one reading last said.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Outcome {
    #[default]
    NotYet,
    Value(f64),
    Unavailable(String),
    Failed(String),
}

impl Outcome {
    fn text(&self) -> String {
        match self {
            Self::NotYet => String::from("not read yet"),
            Self::Value(value) => format!("{value:.1}"),
            Self::Unavailable(reason) => format!("unavailable: {reason}"),
            Self::Failed(error) => format!("failed: {error}"),
        }
    }
}

/// Whether `next` is worth a log line after `previous`: a change of kind or of text is, a new value after a value is
/// not, and the first value is not.
#[must_use]
pub fn is_news(previous: &Outcome, next: &Outcome) -> bool {
    match (previous, next) {
        (Outcome::Value(_) | Outcome::NotYet, Outcome::Value(_)) => false,
        _ => previous != next,
    }
}

/// Keeps `next` as what the reading said, and logs it when it is news.
fn remember(said: &mut Outcome, next: Outcome, reading: &str) {
    if is_news(said, &next) {
        match &next {
            Outcome::Failed(error) => tracing::warn!(reading, error, "reading failed"),
            Outcome::Unavailable(reason) => tracing::info!(reading, reason, "reading unavailable"),
            Outcome::Value(_) => tracing::info!(reading, "reading recovered"),
            Outcome::NotYet => {}
        }
    }
    *said = next;
}

/// One reading as the diagnostics report lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingReport {
    pub reading: &'static str,
    pub outcome: String,
    pub details: Option<String>,
}

/// The text lines to draw for one round of readings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayFrame {
    pub lines: Vec<String>,
}

/// Reads every sensor and renders the latest readings as overlay lines.
pub struct Sampler<C> {
    sensors: Sensors,
    gpus: Box<dyn GpuSensors>,
    /// The adapter the current GPU sensors were opened for.
    gpu: Option<String>,
    clock: C,
    layout: Layout,
    snapshot: Snapshot,
    outcomes: Outcomes,
}

/// What each sensor's last read said, for the report.
#[derive(Default)]
struct Outcomes {
    fps: Outcome,
    cpu_load: Outcome,
    cpu_temp: Outcome,
    gpu_load: Outcome,
    gpu_temp: Outcome,
    memory_load: Outcome,
    memory_temp: Outcome,
}

/// What one read did to its slot, which decides what happens to the extras that came with it.
#[derive(Clone, Copy)]
enum ReadEffect {
    Fresh,
    Kept,
    Cleared,
}

impl<C: Clock> Sampler<C> {
    /// `sensors` already hold the GPU pair for `settings.gpu`; `gpus` opens another when the settings change it.
    pub fn new(sensors: Sensors, gpus: Box<dyn GpuSensors>, clock: C, settings: &Settings) -> Self {
        Self {
            sensors,
            gpus,
            gpu: settings.gpu.clone(),
            clock,
            layout: layout_for(settings),
            snapshot: Snapshot::default(),
            outcomes: Outcomes::default(),
        }
    }

    /// Takes new lines, labels, and interval, and follows another GPU when the settings name one.
    pub fn apply(&mut self, settings: &Settings) {
        self.layout = layout_for(settings);
        if settings.gpu != self.gpu {
            let pair = self.gpus.open(settings.gpu.as_deref());
            self.sensors.gpu_load = pair.load;
            self.sensors.gpu_temp = pair.temp;
            self.gpu.clone_from(&settings.gpu);
            // The old adapter's numbers must not show under the new one.
            let snapshot = &mut self.snapshot;
            snapshot.gpu_load = None;
            snapshot.gpu_temp = None;
            snapshot.gpu_fan_rpm = None;
            snapshot.gpu_memory_mhz = None;
            snapshot.gpu_video_load = None;
            self.outcomes.gpu_load = Outcome::NotYet;
            self.outcomes.gpu_temp = Outcome::NotYet;
        }
    }

    /// Reads every sensor and every extra once, shown or not, and renders the latest readings.
    pub fn sample(&mut self) -> OverlayFrame {
        let now = self.clock.now();
        let (sensors, snapshot) = (&mut self.sensors, &mut self.snapshot);
        let outcome = update(
            &mut snapshot.fps,
            &mut self.outcomes.fps,
            "fps",
            sensors.fps.as_mut(),
            now,
        );
        if let Some(extras) = extras_after(outcome, sensors.fps.as_ref()) {
            set_extra(&mut snapshot.worst_frame_ms, extras.worst_frame_ms, now);
        }
        update(
            &mut snapshot.cpu_load,
            &mut self.outcomes.cpu_load,
            "cpu_load",
            sensors.cpu_load.as_mut(),
            now,
        );
        update(
            &mut snapshot.cpu_temp,
            &mut self.outcomes.cpu_temp,
            "cpu_temp",
            sensors.cpu_temp.as_mut(),
            now,
        );
        let outcome = update(
            &mut snapshot.gpu_load,
            &mut self.outcomes.gpu_load,
            "gpu_load",
            sensors.gpu_load.as_mut(),
            now,
        );
        if let Some(extras) = extras_after(outcome, sensors.gpu_load.as_ref()) {
            set_extra(&mut snapshot.gpu_video_load, extras.gpu_video_load, now);
        }
        let outcome = update(
            &mut snapshot.gpu_temp,
            &mut self.outcomes.gpu_temp,
            "gpu_temp",
            sensors.gpu_temp.as_mut(),
            now,
        );
        if let Some(extras) = extras_after(outcome, sensors.gpu_temp.as_ref()) {
            set_extra(&mut snapshot.gpu_fan_rpm, extras.gpu_fan_rpm, now);
            set_extra(&mut snapshot.gpu_memory_mhz, extras.gpu_memory_mhz, now);
        }
        let outcome = update(
            &mut snapshot.memory_load,
            &mut self.outcomes.memory_load,
            "memory_load",
            sensors.memory_load.as_mut(),
            now,
        );
        if let Some(extras) = extras_after(outcome, sensors.memory_load.as_ref()) {
            set_extra(&mut snapshot.memory_used_gb, extras.memory_used_gb, now);
        }
        let outcome = update(
            &mut snapshot.memory_temp,
            &mut self.outcomes.memory_temp,
            "memory_temp",
            sensors.memory_temp.as_mut(),
            now,
        );
        if let Some(extras) = extras_after(outcome, sensors.memory_temp.as_ref()) {
            snapshot.memory_sticks = extras.stick_celsius.map(|celsius| Sticks { celsius, at: now });
        }
        OverlayFrame {
            lines: self.layout.render(&self.snapshot, now),
        }
    }

    /// Every reading as it last stood: the seven sensors, then the extras whether or not a line shows them. Reads
    /// no hardware.
    pub fn report(&self) -> Vec<ReadingReport> {
        let (sensors, outcomes) = (&self.sensors, &self.outcomes);
        let sensor = |reading, outcome: &Outcome, sensor: &dyn Sensor| ReadingReport {
            reading,
            outcome: outcome.text(),
            details: sensor.details(),
        };
        let mut report = vec![
            sensor("FPS", &outcomes.fps, sensors.fps.as_ref()),
            sensor("CPU load", &outcomes.cpu_load, sensors.cpu_load.as_ref()),
            sensor("CPU temperature", &outcomes.cpu_temp, sensors.cpu_temp.as_ref()),
            sensor("GPU load", &outcomes.gpu_load, sensors.gpu_load.as_ref()),
            sensor("GPU temperature", &outcomes.gpu_temp, sensors.gpu_temp.as_ref()),
            sensor("Memory load", &outcomes.memory_load, sensors.memory_load.as_ref()),
            sensor(
                "Memory temperature",
                &outcomes.memory_temp,
                sensors.memory_temp.as_ref(),
            ),
        ];
        report.extend(self.extras_report());
        report
    }

    fn extras_report(&self) -> [ReadingReport; 6] {
        let (snapshot, now, window) = (&self.snapshot, self.clock.now(), self.layout.interval.saturating_mul(3));
        let number = |reading, sample: Option<Sample>| {
            let value = sample.filter(|s| now.duration_since(s.at) <= window).map(|s| s.value);
            extra(reading, value.map(|value| format!("{value:.1}")))
        };
        let sticks = snapshot
            .memory_sticks
            .as_ref()
            .filter(|s| now.duration_since(s.at) <= window && !s.celsius.is_empty())
            .map(|s| {
                s.celsius
                    .iter()
                    .map(|c| format!("{c:.1}"))
                    .collect::<Vec<_>>()
                    .join("/")
            });
        [
            number("Worst frame (ms)", snapshot.worst_frame_ms),
            number("GPU fan (rpm)", snapshot.gpu_fan_rpm),
            number("GPU VRAM clock (MHz)", snapshot.gpu_memory_mhz),
            number("GPU video load", snapshot.gpu_video_load),
            number("Memory used (GB)", snapshot.memory_used_gb),
            extra("Memory sticks (°C)", sticks),
        ]
    }

    pub fn interval(&self) -> Duration {
        self.layout.interval.unsigned_abs()
    }
}

fn layout_for(settings: &Settings) -> Layout {
    let mut layout = layout_of(settings);
    layout.interval = clamp_interval(layout.interval);
    layout
}

/// `Unavailable` clears the reading; a failed or non-finite read keeps the previous sample until it is stale.
fn update(
    slot: &mut Option<Sample>,
    said: &mut Outcome,
    reading: &str,
    sensor: &mut dyn Sensor,
    now: Timestamp,
) -> ReadEffect {
    match sensor.read() {
        Ok(value) if value.is_finite() => {
            remember(said, Outcome::Value(value), reading);
            *slot = Some(Sample { value, at: now });
            ReadEffect::Fresh
        }
        Ok(value) => {
            remember(
                said,
                Outcome::Failed(String::from("the reading was not a finite number")),
                reading,
            );
            tracing::debug!(?value, reading, "non-finite reading, keeping the previous sample");
            ReadEffect::Kept
        }
        Err(ReadError::Unavailable(reason)) => {
            remember(said, Outcome::Unavailable(reason), reading);
            *slot = None;
            ReadEffect::Cleared
        }
        Err(e @ ReadError::Failed(_)) => {
            remember(said, Outcome::Failed(failure_text(&e)), reading);
            tracing::debug!(error = ?e, reading, "read failed, keeping the previous sample");
            ReadEffect::Kept
        }
    }
}

/// The error and each cause under it on one line, which is how a report reads best.
fn failure_text(error: &ReadError) -> String {
    let mut texts = Vec::new();
    let mut cause = error.source();
    while let Some(next) = cause {
        texts.push(next.to_string());
        cause = next.source();
    }
    if texts.is_empty() {
        texts.push(error.to_string());
    }
    texts.join(": ")
}

/// The extras to apply after a read: the sensor's own after an `Ok`, none at all after `Unavailable`, and `None`
/// (keep what is there until stale) after a failure.
fn extras_after(outcome: ReadEffect, sensor: &dyn Sensor) -> Option<Extras> {
    match outcome {
        ReadEffect::Fresh => Some(sensor.extras()),
        ReadEffect::Cleared => Some(Extras::default()),
        ReadEffect::Kept => None,
    }
}

fn extra(reading: &'static str, text: Option<String>) -> ReadingReport {
    ReadingReport {
        reading,
        outcome: text.unwrap_or_else(|| String::from("not reported")),
        details: None,
    }
}

fn set_extra(slot: &mut Option<Sample>, value: Option<f64>, now: Timestamp) {
    *slot = value.filter(|v| v.is_finite()).map(|value| Sample { value, at: now });
}

#[cfg(test)]
mod tests;
