use std::path::Path;
use std::time::{Duration, Instant};

use monitor_application::ReadError;
use platform_win32::Win32Error;
use platform_win32::mutex::GlobalMutex;
use platform_win32::pawnio::PawnError;

use super::{load_module, open_failure, with_lock};

#[test]
fn a_missing_module_file_is_unavailable_and_says_how_to_get_it() {
    let reply = load_module(Path::new("no-such-dir"), "AMDFamily17.bin");

    assert!(
        matches!(reply, Err(ReadError::Unavailable(ref why)) if why.contains("cargo xtask modules")),
        "{reply:?}"
    );
}

#[test]
fn what_the_machine_lacks_is_unavailable() {
    for lacking in [
        PawnError::NotInstalled,
        PawnError::AccessDenied,
        PawnError::NotSupported,
    ] {
        let reason = lacking.to_string();

        let reply = open_failure(lacking, "x.bin");

        assert!(
            matches!(reply, ReadError::Unavailable(ref why) if *why == reason),
            "{reply:?}"
        );
    }
}

#[test]
fn any_other_load_problem_is_a_failure() {
    for other in [PawnError::NameTooLong, PawnError::Io(Win32Error::from_thread())] {
        let reply = open_failure(other, "x.bin");

        assert!(matches!(reply, ReadError::Failed(_)), "{reply:?}");
    }
}

#[test]
fn a_lock_that_stays_held_fails_the_read_within_a_second_and_skips_the_body() {
    let name = "Local\\overlay-monitor-test-pawn-support-lock";
    let held = GlobalMutex::acquire(name, Duration::from_millis(100)).unwrap().unwrap();

    let started = Instant::now();
    let reply = std::thread::scope(|s| {
        s.spawn(|| with_lock(name, || -> Result<(), ReadError> { panic!("the body must not run") }))
            .join()
            .unwrap()
    });
    let waited = started.elapsed();
    drop(held);

    assert!(matches!(reply, Err(ReadError::Failed(_))), "{reply:?}");
    assert!(waited < Duration::from_secs(1), "{waited:?}");
}

#[test]
fn a_free_lock_runs_the_body() {
    let reply = with_lock::<f64>("Local\\overlay-monitor-test-free-lock", || Ok(3.0));

    assert!(matches!(reply, Ok(v) if (v - 3.0).abs() < f64::EPSILON), "{reply:?}");
}
