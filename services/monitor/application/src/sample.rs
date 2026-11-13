use std::time::Duration;

use monitor_domain::{Layout, Sample, Snapshot};
use platform_clock::{Clock, Timestamp};

use crate::settings::layout_of;
use crate::{ReadError, Sensor, Sensors, Settings};

/// The text lines to draw for one round of readings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayFrame {
    pub lines: Vec<String>,
}

/// Reads every sensor and renders the latest readings as overlay lines.
pub struct Sampler<C> {
    sensors: Sensors,
    clock: C,
    layout: Layout,
    snapshot: Snapshot,
}

impl<C: Clock> Sampler<C> {
    pub fn new(sensors: Sensors, clock: C) -> Self {
        Self {
            sensors,
            clock,
            layout: layout_of(&Settings::default()),
            snapshot: Snapshot::default(),
        }
    }

    /// Reads every sensor once and renders the latest readings.
    pub fn sample(&mut self) -> OverlayFrame {
        let now = self.clock.now();
        let (sensors, snapshot) = (&mut self.sensors, &mut self.snapshot);
        update(&mut snapshot.fps, "fps", sensors.fps.as_mut(), now);
        update(&mut snapshot.cpu_load, "cpu_load", sensors.cpu_load.as_mut(), now);
        update(&mut snapshot.cpu_temp, "cpu_temp", sensors.cpu_temp.as_mut(), now);
        update(&mut snapshot.gpu_load, "gpu_load", sensors.gpu_load.as_mut(), now);
        update(&mut snapshot.gpu_temp, "gpu_temp", sensors.gpu_temp.as_mut(), now);
        update(
            &mut snapshot.memory_load,
            "memory_load",
            sensors.memory_load.as_mut(),
            now,
        );
        update(
            &mut snapshot.memory_temp,
            "memory_temp",
            sensors.memory_temp.as_mut(),
            now,
        );
        OverlayFrame {
            lines: self.layout.render(&self.snapshot, now),
        }
    }

    pub fn interval(&self) -> Duration {
        self.layout.interval.unsigned_abs()
    }
}

/// `Unavailable` clears the reading; a failed or non-finite read keeps the previous sample until it is stale.
fn update(slot: &mut Option<Sample>, reading: &str, sensor: &mut dyn Sensor, now: Timestamp) {
    match sensor.read() {
        Ok(value) if value.is_finite() => *slot = Some(Sample { value, at: now }),
        Ok(value) => tracing::debug!(?value, reading, "non-finite reading, keeping the previous sample"),
        Err(ReadError::Unavailable(_)) => *slot = None,
        Err(e @ ReadError::Failed(_)) => {
            tracing::debug!(error = ?e, reading, "read failed, keeping the previous sample");
        }
    }
}

#[cfg(test)]
mod tests;
