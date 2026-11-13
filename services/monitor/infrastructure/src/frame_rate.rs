//! Frames per second of the app in front, counted from the graphics kernel's present events the way `PresentMon`
//! tracks them, so nothing is injected into the game.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use monitor_application::{ReadError, Sensor};
use platform_win32::etw::{EventInfo, Provider};
use platform_win32::foreground::foreground_process_id;

/// Microsoft-Windows-DxgKrnl, the graphics kernel.
const DXGKRNL: u128 = 0x802e_c45a_1e99_4b83_9920_87c9_8277_ba9d;
/// `Present_Info`, logged as the kernel's present call returns.
const PRESENT_INFO: u16 = 0x00b8;
/// Windows 11's `DxgKrnl` files its present and vsync events, not its far busier scheduling ones, under this keyword.
const PRESENT_KEYWORD: u64 = 0x0800_0000;
/// `Present_Info` is logged at level 0 (always); level 1 keeps the provider's informational events out.
const CRITICAL: u8 = 1;

// Only DxgKrnl Present_Info (0xB8): DirectX 9 to 12, Vulkan, and OpenGL end every frame's kernel present call with exactly one (PresentMon reads it as a present's last event; measured equal to the frames each API presented, windowed and borderless), while the DXGI and D3D9 runtime events and the kernel's present-history, Blit, and Flip events mark the same frames again (AMD's Vulkan and borderless OpenGL even go through DXGI), so they are left out.
pub const PRESENT_PROVIDERS: &[Provider] = &[Provider {
    guid: DXGKRNL,
    level: CRITICAL,
    match_any_keyword: PRESENT_KEYWORD,
}];

/// Whether `event` is one frame presented by its process.
#[must_use]
pub fn is_present(event: &EventInfo) -> bool {
    event.provider == DXGKRNL && event.id == PRESENT_INFO
}

/// Frames per process since the last `take`. The trace consumer records; the sensor takes.
#[derive(Debug, Default)]
pub struct FrameCounter {
    counts: Mutex<BTreeMap<u32, u32>>,
}

impl FrameCounter {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    pub fn record(&self, process_id: u32) {
        let mut counts = self.lock();
        let count = counts.entry(process_id).or_default();
        *count = count.saturating_add(1);
    }

    /// The counts so far, leaving them at zero.
    pub fn take(&self) -> BTreeMap<u32, u32> {
        std::mem::take(&mut *self.lock())
    }

    /// The counts are plain numbers, always whole, so a panic elsewhere while holding the lock does not spoil them.
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<u32, u32>> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Frames per second of the process that owns the foreground window, over the time since the last read.
#[derive(Debug)]
pub struct FrameRate {
    counter: Arc<FrameCounter>,
    own_process_id: u32,
    last_read: Instant,
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
        }
    }
}

impl Sensor for FrameRate {
    fn read(&mut self) -> Result<f64, ReadError> {
        let now = Instant::now();
        let elapsed = now.duration_since(std::mem::replace(&mut self.last_read, now));
        let counts = self.counter.take();
        let count = rate_for(foreground_process_id(), self.own_process_id, &counts)?;
        Ok(frames_per_second(count, elapsed))
    }
}

/// The foreground app's frame count. Absent frames are `Unavailable`, so the overlay shows `—` at once, never 0 or
/// the rate of the app that was in front before.
fn rate_for(foreground: Option<u32>, own: u32, counts: &BTreeMap<u32, u32>) -> Result<u32, ReadError> {
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
