use std::collections::BTreeMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use monitor_application::{ReadError, Sensor};
use platform_win32::etw::{EventInfo, start_session};
use platform_win32::testing::present_frames;

use super::{FrameCounter, FrameRate, FrameStats, PRESENT_PROVIDERS, frames_per_second, is_present, rate_for};

const DXGKRNL: u128 = 0x802e_c45a_1e99_4b83_9920_87c9_8277_ba9d;
const DXGI: u128 = 0xca11_c036_0102_4a2d_a6ad_f03c_fed5_d3c9;

fn event(provider: u128, id: u16) -> EventInfo {
    EventInfo {
        provider,
        id,
        process_id: 7,
        timestamp_ns: 0,
    }
}

#[test]
fn frames_per_second_is_count_over_elapsed_seconds() {
    assert!((frames_per_second(144, Duration::from_millis(1000)) - 144.0).abs() < 1e-9);
    assert!((frames_per_second(30, Duration::from_millis(500)) - 60.0).abs() < 1e-9);
}

#[test]
fn each_process_has_its_own_count_and_take_resets_it() {
    let c = FrameCounter::new();
    c.record(7, 0);
    c.record(7, MS);
    c.record(9, 0);
    let counts: BTreeMap<u32, u32> = c.take().into_iter().map(|(pid, stats)| (pid, stats.count)).collect();
    assert_eq!(counts, BTreeMap::from([(7, 2), (9, 1)]));
    assert!(c.take().is_empty());
}

#[test]
fn the_app_in_front_has_its_own_count() {
    let counts = BTreeMap::from([(5, stats(60)), (9, stats(30))]);
    assert!(matches!(
        rate_for(Some(5), 42, &counts),
        Ok(FrameStats { count: 60, .. })
    ));
}

#[test]
fn the_overlay_itself_has_no_frame_rate() {
    assert!(matches!(
        rate_for(Some(42), 42, &BTreeMap::from([(42, stats(60))])),
        Err(ReadError::Unavailable(_))
    ));
}

#[test]
fn an_app_without_frames_has_no_frame_rate() {
    assert!(matches!(
        rate_for(Some(5), 42, &BTreeMap::new()),
        Err(ReadError::Unavailable(_))
    ));
    assert!(matches!(
        rate_for(None, 42, &BTreeMap::new()),
        Err(ReadError::Unavailable(_))
    ));
}

#[test]
fn only_the_kernels_present_event_is_a_frame() {
    assert!(
        is_present(&event(DXGKRNL, 0xb8)),
        "Present_Info ends every kernel present"
    );
    assert!(!is_present(&event(DXGKRNL, 0x11)), "a vsync is not a frame");
    assert!(
        !is_present(&event(DXGKRNL, 0xd7)),
        "PresentHistoryDetailed_Start repeats a windowed frame the kernel present already counts"
    );
    assert!(
        !is_present(&event(DXGI, 0x2a)),
        "DXGI's Present_Start repeats a frame the kernel present already counts"
    );
}

#[test]
fn a_read_right_after_the_last_is_too_short_to_count_and_keeps_the_last_value() {
    let mut rate = FrameRate::new(FrameCounter::new(), 42);
    assert!(
        matches!(rate.read(), Err(ReadError::Failed(_))),
        "one frame in a few milliseconds would read as hundreds per second"
    );
}

#[test]
fn present_info_is_asked_for_under_the_keyword_every_windows_logs_it_with_and_alone() {
    const BASE: u64 = 0x1;
    let [kernel] = PRESENT_PROVIDERS else {
        panic!("one provider, the graphics kernel: {PRESENT_PROVIDERS:?}");
    };
    assert_eq!(kernel.guid, DXGKRNL);
    assert_ne!(
        kernel.match_any_keyword & BASE,
        0,
        "before Windows 11, Present_Info carries only the Base keyword"
    );
    assert_eq!(
        kernel.event_ids,
        [0xb8],
        "Base also covers the busy scheduling events, so only Present_Info is let in"
    );
}

fn wait(time: Duration) {
    let (_still_open, never) = mpsc::channel::<()>();
    assert_eq!(never.recv_timeout(time), Err(RecvTimeoutError::Timeout));
}

// The compositor's own frames are not an app's presents, and nothing else may draw during the test, so the test
// presents frames of its own through the compositor and checks that each is counted once.
#[test]
#[ignore = "needs an elevated terminal"]
fn the_desktop_compositor_presents_frames() {
    let (session, consumer) = start_session("OverlayMonitorFrameTest", PRESENT_PROVIDERS).unwrap();
    let counter = FrameCounter::new();
    let mut count = |event: &EventInfo| {
        if is_present(event) {
            counter.record(event.process_id, event.timestamp_ns);
        }
    };

    let presented = std::thread::scope(|s| {
        let consuming = s.spawn(|| consumer.run(&mut count));
        // The consumer connects within milliseconds; this leaves it ample time before the first frame.
        wait(Duration::from_millis(500));
        let presented = present_frames(10).unwrap();
        wait(Duration::from_millis(1500));
        session.stop().unwrap();
        consuming.join().unwrap().unwrap();
        presented
    });

    assert!(presented > 0, "the test window presented no frame");
    assert_eq!(
        counter.take().get(&std::process::id()).map(|stats| stats.count),
        Some(presented),
        "each frame this test presented is counted once"
    );
}

const MS: u64 = 1_000_000;

fn stats(count: u32) -> FrameStats {
    FrameStats { count, worst_ns: None }
}

fn worst(counter: &FrameCounter) -> Option<u64> {
    counter.take().get(&7).and_then(|stats| stats.worst_ns)
}

#[test]
fn the_worst_frame_is_the_longest_gap_between_two_presents() {
    let c = FrameCounter::new();
    for at in [0, 16_700_000, 50_000_000, 66_700_000] {
        c.record(7, at);
    }
    assert_eq!(worst(&c), Some(33_300_000));
}

#[test]
fn a_gap_over_a_second_is_a_pause_not_a_frame() {
    let c = FrameCounter::new();
    c.record(7, 0);
    c.record(7, 20 * MS);
    c.record(7, 2_000 * MS);
    assert_eq!(worst(&c), Some(20 * MS), "the pause is not the worst frame");
}

#[test]
fn one_present_has_no_worst_frame() {
    let c = FrameCounter::new();
    c.record(7, 5 * MS);
    let taken = c.take();
    assert_eq!(
        taken.get(&7),
        Some(&FrameStats {
            count: 1,
            worst_ns: None
        })
    );
}

#[test]
fn a_gap_across_two_readings_counts() {
    let c = FrameCounter::new();
    c.record(7, 0);
    c.record(7, 10 * MS);
    c.take();
    c.record(7, 40 * MS);
    assert_eq!(worst(&c), Some(30 * MS));
}
