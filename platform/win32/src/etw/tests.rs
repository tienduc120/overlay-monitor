use super::{TraceConsumer, TraceSession, start_session};

fn shared_across_threads<T: Send + Sync>() {}
fn moved_across_threads<T: Send>() {}

#[test]
fn the_session_is_shared_and_the_consumer_moved_to_the_thread_that_runs_it() {
    shared_across_threads::<TraceSession>();
    moved_across_threads::<TraceConsumer>();
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
