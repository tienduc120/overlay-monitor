//! Frames per second of the app in front, counted from the graphics kernel's present events the way `PresentMon`
//! tracks them, so nothing is injected into the game.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use monitor_application::{Extras, ReadError, Sensor};
use platform_win32::etw::{EventInfo, Provider};
use platform_win32::foreground::foreground_process_id;

/// Microsoft-Windows-DxgKrnl, the graphics kernel.
const DXGKRNL: u128 = 0x802e_c45a_1e99_4b83_9920_87c9_8277_ba9d;
/// `Present_Info`, logged as the kernel's present call returns.
const PRESENT_INFO: u16 = 0x00b8;
/// `Base` and `Present`: `Present_Info` carries only `Base` before Windows 11 and both from it on. `Base` also covers
/// the busy GPU scheduling events, which the event id filter keeps out, as `PresentMon` does.
const BASE_AND_PRESENT_KEYWORDS: u64 = 0x0800_0001;
/// `Present_Info` is logged at level 0 (always); level 1 keeps the provider's informational events out.
const CRITICAL: u8 = 1;

// Only DxgKrnl Present_Info (0xB8): DirectX 9 to 12, Vulkan, and OpenGL end every frame's kernel present call with exactly one (PresentMon reads it as a present's last event; measured equal to the frames each API presented, windowed and borderless), while the DXGI and D3D9 runtime events and the kernel's present-history, Blit, and Flip events mark the same frames again (AMD's Vulkan and borderless OpenGL even go through DXGI), so they are left out.
pub const PRESENT_PROVIDERS: &[Provider] = &[Provider {
    guid: DXGKRNL,
    level: CRITICAL,
    match_any_keyword: BASE_AND_PRESENT_KEYWORDS,
    event_ids: &[PRESENT_INFO],
}];

/// Whether `event` is one frame presented by its process.
#[must_use]
pub fn is_present(event: &EventInfo) -> bool {
    event.provider == DXGKRNL && event.id == PRESENT_INFO
}

/// A gap between two presents longer than this is the app pausing (a loading screen, a minimised window), not a frame.
const LONGEST_FRAME_NS: u64 = 1_000_000_000;

/// One process's frames since the last `take`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub count: u32,
    /// The longest gap between two consecutive presents; `None` with fewer than two.
    pub worst_ns: Option<u64>,
}

#[derive(Debug, Default)]
struct Tally {
    count: u32,
    worst_ns: Option<u64>,
    /// The last present's time, kept across a `take` so a gap over two readings still counts.
    last_ns: Option<u64>,
}

/// Frames per process since the last `take`. The trace consumer records; the sensor takes.
#[derive(Debug, Default)]
pub struct FrameCounter {
    tallies: Mutex<BTreeMap<u32, Tally>>,
}

impl FrameCounter {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Records one present of `process_id` at `at_ns` on the trace's clock.
    pub fn record(&self, process_id: u32, at_ns: u64) {
        let mut tallies = self.lock();
        let tally = tallies.entry(process_id).or_default();
        tally.count = tally.count.saturating_add(1);
        if let Some(gap) = tally
            .last_ns
            .map(|last| at_ns.saturating_sub(last))
            .filter(|&gap| gap <= LONGEST_FRAME_NS)
        {
            tally.worst_ns = tally.worst_ns.max(Some(gap));
        }
        tally.last_ns = Some(at_ns);
    }

    /// The stats so far, leaving the counts at zero. A process that presented nothing since the last `take` is
    /// forgotten, so the map does not grow with every process that ever drew.
    pub fn take(&self) -> BTreeMap<u32, FrameStats> {
        let mut tallies = self.lock();
        let taken = tallies
            .iter()
            .filter(|(_, tally)| tally.count > 0)
            .map(|(&pid, tally)| {
                let stats = FrameStats {
                    count: tally.count,
                    worst_ns: tally.worst_ns,
                };
                (pid, stats)
            })
            .collect();
        tallies.retain(|_, tally| tally.count > 0);
        for tally in tallies.values_mut() {
            tally.count = 0;
            tally.worst_ns = None;
        }
        taken
    }

    /// The tallies are plain numbers, always whole, so a panic elsewhere while holding the lock does not spoil them.
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<u32, Tally>> {
        self.tallies.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Nanoseconds as milliseconds, counted in whole microseconds: a `u32` converts to `f64` exactly, and a frame time
/// shows a tenth of a millisecond.
fn ns_to_ms(ns: u64) -> f64 {
    let us = u32::try_from(ns / 1_000).unwrap_or(u32::MAX);
    f64::from(us) / 1_000.0
}

/// Below this, one frame more or less swings the rate by 10 or more per second.
const SHORTEST_READING: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
enum FrameRateError {
    #[error("only {0:?} passed since the last reading, too short to count frames over")]
    TooSoon(Duration),
}

/// Frames per second of the process that owns the foreground window, over the time since the last read.
#[derive(Debug)]
pub struct FrameRate {
    counter: Arc<FrameCounter>,
    own_process_id: u32,
    last_read: Instant,
    worst_frame_ms: Option<f64>,
}

impl FrameRate {
    /// `own_process_id` is the overlay's: in front, it has no frame rate to show.
    #[must_use]
    pub fn new(counter: Arc<FrameCounter>, own_process_id: u32) -> Self {
        // Frames counted before now belong to no reading.
        counter.take();
        Self {
            counter,
            own_process_id,
            last_read: Instant::now(),
            worst_frame_ms: None,
        }
    }
}

impl Sensor for FrameRate {
    fn read(&mut self) -> Result<f64, ReadError> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_read);
        if elapsed < SHORTEST_READING {
            // The frames stay counted, so the next reading covers this stretch too.
            return Err(ReadError::Failed(Box::new(FrameRateError::TooSoon(elapsed))));
        }
        self.last_read = now;
        let counts = self.counter.take();
        let stats = rate_for(foreground_process_id(), self.own_process_id, &counts)?;
        self.worst_frame_ms = stats.worst_ns.map(ns_to_ms);
        Ok(frames_per_second(stats.count, elapsed))
    }

    fn extras(&self) -> Extras {
        Extras {
            worst_frame_ms: self.worst_frame_ms,
            ..Extras::default()
        }
    }
}

/// The foreground app's frame count. Absent frames are `Unavailable`, so the overlay shows `—` at once, never 0 or
/// the rate of the app that was in front before.
fn rate_for(foreground: Option<u32>, own: u32, counts: &BTreeMap<u32, FrameStats>) -> Result<FrameStats, ReadError> {
    let app = foreground.ok_or_else(|| ReadError::Unavailable("no window is in front".to_owned()))?;
    if app == own {
        return Err(ReadError::Unavailable("the overlay itself is in front".to_owned()));
    }
    counts
        .get(&app)
        .copied()
        .ok_or_else(|| ReadError::Unavailable("the app in front presented no frames".to_owned()))
}

#[must_use]
pub fn frames_per_second(count: u32, elapsed: Duration) -> f64 {
    f64::from(count) / elapsed.as_secs_f64()
}

#[cfg(test)]
mod tests;
