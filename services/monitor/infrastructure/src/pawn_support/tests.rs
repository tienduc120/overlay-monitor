use std::path::Path;
use std::time::{Duration, Instant};

use monitor_application::testing::FakeSensor;
use monitor_application::{PawnIoStatus, ReadError, Sensor};
use platform_win32::Win32Error;
use platform_win32::mutex::GlobalMutex;
use platform_win32::pawnio::{PawnError, PawnVersion};

use super::{classify, load_module, open_failure, reopening, with_lock};

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

fn version(major: u8, minor: u8, patch: u8) -> PawnVersion {
    PawnVersion { major, minor, patch }
}

#[test]
fn the_floor_is_2_1_0() {
    assert_eq!(
        classify(Ok(version(2, 0, 9))),
        PawnIoStatus::TooOld {
            version: Some("2.0.9".into())
        }
    );
    for ok in [version(2, 1, 0), version(2, 2, 0)] {
        assert_eq!(
            classify(Ok(ok)),
            PawnIoStatus::Installed {
                version: ok.to_string()
            }
        );
    }
}

#[test]
fn a_driver_without_the_version_call_is_too_old() {
    assert_eq!(
        classify(Err(PawnError::NoVersion)),
        PawnIoStatus::TooOld { version: None }
    );
}

#[test]
fn access_denied_means_installed_but_not_elevated() {
    assert_eq!(classify(Err(PawnError::AccessDenied)), PawnIoStatus::NeedsAdmin);
    assert_eq!(classify(Err(PawnError::NotInstalled)), PawnIoStatus::NotInstalled);
    assert!(matches!(
        classify(Err(PawnError::NameTooLong)),
        PawnIoStatus::Unanswered { .. }
    ));
}

#[test]
fn an_install_is_picked_up_on_a_later_read() {
    let mut answers = vec![
        PawnIoStatus::Installed {
            version: "2.2.0".into(),
        },
        PawnIoStatus::NotInstalled,
    ];
    let mut sensor = reopening(
        move || answers.pop().unwrap_or(PawnIoStatus::NotInstalled),
        || Ok(FakeSensor::fixed(50.0)),
    );

    let first = sensor.read();
    let second = sensor.read();

    assert!(
        matches!(first, Err(ReadError::Unavailable(ref why)) if why == "not installed"),
        "{first:?}"
    );
    assert!(matches!(second, Ok(v) if (v - 50.0).abs() < f64::EPSILON), "{second:?}");
}

#[test]
fn a_module_that_refuses_this_machine_is_not_tried_again() {
    let mut opens = 0;
    let mut sensor = reopening(
        || PawnIoStatus::Installed {
            version: "2.2.0".into(),
        },
        move || -> Result<FakeSensor, ReadError> {
            opens += 1;
            assert_eq!(opens, 1, "open must run once");
            Err(ReadError::Unavailable(
                "the module does not support this machine".into(),
            ))
        },
    );

    for _ in 0..2 {
        let reply = sensor.read();

        assert!(
            matches!(reply, Err(ReadError::Unavailable(ref why)) if why == "the module does not support this machine"),
            "{reply:?}"
        );
    }
}

#[test]
fn a_too_old_driver_never_loads_a_module() {
    let mut sensor = reopening(
        || PawnIoStatus::TooOld { version: None },
        || -> Result<FakeSensor, ReadError> { panic!("no module may load") },
    );

    let reply = sensor.read();

    assert!(
        matches!(reply, Err(ReadError::Unavailable(ref why)) if why.contains("older than 2.1.0")),
        "{reply:?}"
    );
}

#[test]
fn each_status_says_what_to_do_about_it() {
    let said = |status: PawnIoStatus| status.describe();

    assert_eq!(
        said(PawnIoStatus::Installed {
            version: "2.2.0".into()
        }),
        "installed, version 2.2.0 ✓"
    );
    assert!(
        said(PawnIoStatus::TooOld {
            version: Some("2.0.1".into())
        })
        .starts_with("version 2.0.1 is older than 2.1.0,")
    );
    assert!(said(PawnIoStatus::TooOld { version: None }).starts_with("older than 2.1.0,"));
    assert_eq!(said(PawnIoStatus::NotInstalled), "not installed");
    assert_eq!(
        said(PawnIoStatus::NeedsAdmin),
        "installed, but Overlay Monitor is not running as administrator"
    );
    assert_eq!(
        said(PawnIoStatus::Unanswered { reason: "x".into() }),
        "did not answer: x"
    );
}
