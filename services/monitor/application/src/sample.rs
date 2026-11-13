use std::time::Duration;

use monitor_domain::{Layout, Sample, Snapshot, Sticks, clamp_interval};
use platform_clock::{Clock, Timestamp};

use crate::settings::layout_of;
use crate::{Extras, GpuSensors, ReadError, Sensor, Sensors, Settings};

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
}

/// What one read did to its slot, which decides what happens to the extras that came with it.
#[derive(Clone, Copy)]
enum Outcome {
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
        }
    }

    /// Reads every sensor and every extra once, shown or not, and renders the latest readings.
    pub fn sample(&mut self) -> OverlayFrame {
        let now = self.clock.now();
        let (sensors, snapshot) = (&mut self.sensors, &mut self.snapshot);
        let outcome = update(&mut snapshot.fps, "fps", sensors.fps.as_mut(), now);
        if let Some(extras) = extras_after(outcome, sensors.fps.as_ref()) {
            set_extra(&mut snapshot.worst_frame_ms, extras.worst_frame_ms, now);
        }
        update(&mut snapshot.cpu_load, "cpu_load", sensors.cpu_load.as_mut(), now);
        update(&mut snapshot.cpu_temp, "cpu_temp", sensors.cpu_temp.as_mut(), now);
        let outcome = update(&mut snapshot.gpu_load, "gpu_load", sensors.gpu_load.as_mut(), now);
        if let Some(extras) = extras_after(outcome, sensors.gpu_load.as_ref()) {
            set_extra(&mut snapshot.gpu_video_load, extras.gpu_video_load, now);
        }
        let outcome = update(&mut snapshot.gpu_temp, "gpu_temp", sensors.gpu_temp.as_mut(), now);
        if let Some(extras) = extras_after(outcome, sensors.gpu_temp.as_ref()) {
            set_extra(&mut snapshot.gpu_fan_rpm, extras.gpu_fan_rpm, now);
            set_extra(&mut snapshot.gpu_memory_mhz, extras.gpu_memory_mhz, now);
        }
        let outcome = update(
            &mut snapshot.memory_load,
            "memory_load",
            sensors.memory_load.as_mut(),
            now,
        );
        if let Some(extras) = extras_after(outcome, sensors.memory_load.as_ref()) {
            set_extra(&mut snapshot.memory_used_gb, extras.memory_used_gb, now);
        }
        let outcome = update(
            &mut snapshot.memory_temp,
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
fn update(slot: &mut Option<Sample>, reading: &str, sensor: &mut dyn Sensor, now: Timestamp) -> Outcome {
    match sensor.read() {
        Ok(value) if value.is_finite() => {
            *slot = Some(Sample { value, at: now });
            Outcome::Fresh
        }
        Ok(value) => {
            tracing::debug!(?value, reading, "non-finite reading, keeping the previous sample");
            Outcome::Kept
        }
        Err(ReadError::Unavailable(_)) => {
            *slot = None;
            Outcome::Cleared
        }
        Err(e @ ReadError::Failed(_)) => {
            tracing::debug!(error = ?e, reading, "read failed, keeping the previous sample");
            Outcome::Kept
        }
    }
}

/// The extras to apply after a read: the sensor's own after an `Ok`, none at all after `Unavailable`, and `None`
/// (keep what is there until stale) after a failure.
fn extras_after(outcome: Outcome, sensor: &dyn Sensor) -> Option<Extras> {
    match outcome {
        Outcome::Fresh => Some(sensor.extras()),
        Outcome::Cleared => Some(Extras::default()),
        Outcome::Kept => None,
    }
}

fn set_extra(slot: &mut Option<Sample>, value: Option<f64>, now: Timestamp) {
    *slot = value.filter(|v| v.is_finite()).map(|value| Sample { value, at: now });
}

#[cfg(test)]
mod tests;
