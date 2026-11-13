use std::mem::offset_of;

use windows::Win32::System::Diagnostics::Etw::EVENT_FILTER_EVENT_ID;

use super::{EventIdFilter, TraceConsumer, TraceSession, start_session, stop_session, ticks_to_ns};

fn shared_across_threads<T: Send + Sync>() {}
fn moved_across_threads<T: Send>() {}

#[test]
fn the_session_is_shared_and_the_consumer_moved_to_the_thread_that_runs_it() {
    shared_across_threads::<TraceSession>();
    moved_across_threads::<TraceConsumer>();
}

#[test]
fn an_event_filter_lets_in_its_ids_listed_right_after_its_header() {
    let filter = EventIdFilter::new(&[0xb8, 0x11]).unwrap();

    assert!(filter.header.FilterIn, "the listed ids are let in, not kept out");
    assert_eq!(filter.header.Count, 2);
    assert_eq!(filter.header.Events, [0xb8]);
    assert_eq!(filter.more.first(), Some(&0x11));
    assert_eq!(
        offset_of!(EventIdFilter, more),
        offset_of!(EventIdFilter, header) + offset_of!(EVENT_FILTER_EVENT_ID, Events) + size_of::<u16>(),
        "the second id must sit right after the first, where ETW reads it"
    );
    assert_eq!(filter.size(), 8, "4 bytes of header, then two ids");
}

#[test]
fn a_filter_of_more_ids_than_etw_accepts_is_refused() {
    assert_eq!(EventIdFilter::new(&[0; 64]).ok().map(|f| f.header.Count), Some(64));
    assert_eq!(EventIdFilter::new(&[0; 65]).ok().map(|f| f.header.Count), None);
}

#[test]
#[ignore = "needs an elevated terminal"]
fn starting_a_session_twice_with_one_name_succeeds() {
    let (first, _c1) = start_session("OverlayMonitorTest", &[]).unwrap();
    #[expect(
        clippy::mem_forget,
        reason = "simulates a crashed process that never stopped its session"
    )]
    std::mem::forget(first); // what a crash leaves behind
    let (second, _c2) = start_session("OverlayMonitorTest", &[]).unwrap();
    second.stop().unwrap();
}

#[test]
#[ignore = "needs an elevated terminal"]
fn a_consumer_whose_session_already_stopped_returns_at_once() {
    let (session, consumer) = start_session("OverlayMonitorStopFirstTest", &[]).unwrap();
    session.stop().unwrap();
    consumer.run(&mut |_| {}).unwrap();
}

#[test]
fn ticks_convert_to_nanoseconds() {
    assert_eq!(ticks_to_ns(10_000_000, 10_000_000), 1_000_000_000);
    assert_eq!(
        ticks_to_ns(i64::MAX, 10_000_000),
        u64::MAX,
        "too late to count saturates"
    );
    assert_eq!(ticks_to_ns(-1, 10_000_000), 0, "a negative time is no time");
    assert_eq!(ticks_to_ns(5, 0), 0, "no frequency is no time");
}

#[test]
#[ignore = "needs an elevated terminal"]
fn stopping_a_session_that_does_not_exist_is_fine() {
    let stopped = stop_session("OverlayMonitorTestNoSuchSession");

    assert!(stopped.is_ok(), "{stopped:?}");
}
