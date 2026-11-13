use std::collections::BTreeMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use monitor_application::ReadError;
use platform_win32::etw::{EventInfo, start_session};

use super::{FrameCounter, PRESENT_PROVIDERS, frames_per_second, is_present, rate_for};

const DXGKRNL: u128 = 0x802e_c45a_1e99_4b83_9920_87c9_8277_ba9d;
const DXGI: u128 = 0xca11_c036_0102_4a2d_a6ad_f03c_fed5_d3c9;

fn event(provider: u128, id: u16) -> EventInfo {
    EventInfo {
        provider,
        id,
        process_id: 7,
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
    c.record(7);
    c.record(7);
    c.record(9);
    assert_eq!(c.take(), BTreeMap::from([(7, 2), (9, 1)]));
    assert!(c.take().is_empty());
}

#[test]
fn the_app_in_front_has_its_own_count() {
    let counts = BTreeMap::from([(5, 60), (9, 30)]);
    assert!(matches!(rate_for(Some(5), 42, &counts), Ok(60)));
}

#[test]
fn the_overlay_itself_has_no_frame_rate() {
    assert!(matches!(
        rate_for(Some(42), 42, &BTreeMap::from([(42, 60)])),
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

// The frames come from apps drawing through the compositor, such as the terminal showing this run: the compositor's
// own frames are not an app's presents, so on a desktop where nothing draws for 2 s this sees none.
#[test]
#[ignore = "needs an elevated terminal"]
fn the_desktop_compositor_presents_frames() {
    let (session, consumer) = start_session("OverlayMonitorFrameTest", PRESENT_PROVIDERS).unwrap();
    let mut frames = 0u32;

    std::thread::scope(|s| {
        let consuming = s.spawn(|| consumer.run(&mut |event| frames += u32::from(is_present(event))));
        let (_still_open, two_seconds) = mpsc::channel::<()>();
        assert_eq!(
            two_seconds.recv_timeout(Duration::from_secs(2)),
            Err(RecvTimeoutError::Timeout)
        );
        session.stop().unwrap();
        consuming.join().unwrap().unwrap();
    });

    assert!(frames > 0, "no frame was presented in 2 s");
}
