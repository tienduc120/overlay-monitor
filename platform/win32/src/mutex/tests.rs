use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

use super::GlobalMutex;

#[test]
fn a_held_lock_times_out_instead_of_blocking() {
    let name = "Local\\overlay-monitor-test-lock";
    let held = GlobalMutex::acquire(name, Duration::from_millis(100)).unwrap();
    assert!(held.is_some());
    std::thread::scope(|s| {
        let other = s.spawn(|| GlobalMutex::acquire(name, Duration::from_millis(50)).unwrap().is_none());
        assert!(other.join().unwrap(), "a second thread must give up after the timeout");
    });
}

#[test]
fn dropping_the_guard_releases_the_lock() {
    let name = r"Local\overlay-monitor-test-release-lock";
    let held = GlobalMutex::acquire(name, Duration::from_millis(100)).unwrap();
    drop(held);

    let taken = std::thread::scope(|s| {
        s.spawn(|| {
            GlobalMutex::acquire(name, Duration::from_millis(100))
                .unwrap()
                .is_some()
        })
        .join()
        .unwrap()
    });

    assert!(taken, "a second thread must get the lock once the first guard is gone");
}

/// Creates `Local\overlay-monitor-test-narrow` as another program would: everyone may only wait on it and release it.
fn narrow_mutex() -> HANDLE {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the SDDL string is a NUL-terminated literal and `descriptor` is a live out-parameter.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:(A;;0x100001;;;WD)"),
            SDDL_REVISION_1,
            &raw mut descriptor,
            None,
        )
    }
    .unwrap();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap(),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    // SAFETY: the attributes and the descriptor they point to outlive the call, and the name is a literal.
    let handle = unsafe {
        CreateMutexW(
            Some(&raw const attributes),
            false,
            w!(r"Local\overlay-monitor-test-narrow"),
        )
    }
    .unwrap();
    // SAFETY: the descriptor was allocated by the conversion above and is not used again.
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    handle
}

#[test]
fn a_lock_another_program_made_with_a_narrow_acl_is_opened_instead() {
    let creator = narrow_mutex();

    let taken = GlobalMutex::acquire(r"Local\overlay-monitor-test-narrow", Duration::from_millis(100)).unwrap();

    assert!(taken.is_some());
    drop(taken);
    // SAFETY: the test made this handle and closes it once.
    unsafe { CloseHandle(creator) }.unwrap();
}
