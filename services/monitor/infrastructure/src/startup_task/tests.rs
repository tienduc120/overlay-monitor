use std::fs;
use std::path::Path;
use std::process::Command;

use monitor_application::StartupTask;

use super::*;

const SID: &str = "S-1-5-21-1-2-3-1001";

fn xml() -> String {
    task_xml(Path::new(r"C:\Apps\overlay-monitor.exe"), SID)
}

#[test]
fn the_task_runs_the_exe_at_sign_in_with_highest_privileges() {
    let xml = xml();
    assert!(
        xml.contains(&format!(
            "<LogonTrigger><Enabled>true</Enabled><UserId>{SID}</UserId></LogonTrigger>"
        )),
        "{xml}"
    );
    assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"), "{xml}");
    assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"), "{xml}");
    assert!(
        xml.contains(r"<Exec><Command>C:\Apps\overlay-monitor.exe</Command></Exec>"),
        "{xml}"
    );
}

#[test]
fn the_task_is_never_stopped_after_three_days_or_on_battery() {
    let xml = xml();
    assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"), "{xml}");
    assert!(
        xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"),
        "{xml}"
    );
    assert!(
        xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"),
        "{xml}"
    );
}

#[test]
fn a_path_with_xml_characters_is_escaped() {
    let xml = task_xml(Path::new(r"C:\A&B\<x>.exe"), SID);
    assert!(xml.contains(r"<Command>C:\A&amp;B\&lt;x&gt;.exe</Command>"), "{xml}");
}

#[test]
fn the_file_is_utf16_with_a_byte_order_mark() {
    let bytes = utf16_with_bom("a\u{e9}");
    assert_eq!(bytes, [0xFF, 0xFE, b'a', 0x00, 0xE9, 0x00]);
}

fn refused_delete() -> SchtasksError {
    SchtasksError::Failed {
        action: "/Delete",
        status: String::from("exit code: 1"),
        output: String::from("ERROR: Access is denied."),
    }
}

#[test]
fn a_refused_delete_of_a_task_that_is_still_listed_is_reported() {
    let outcome = removal(Err(refused_delete()), || true);

    assert!(
        matches!(outcome, Err(SchtasksError::Failed { action: "/Delete", .. })),
        "{outcome:?}"
    );
}

#[test]
fn a_refused_delete_of_a_task_that_is_not_there_is_done() {
    removal(Err(refused_delete()), || false).unwrap();
    removal(Ok(()), || panic!("a deleted task needs no look")).unwrap();
}

#[test]
fn schtasks_that_cannot_run_is_reported_whatever_the_list_says() {
    let outcome = removal(Err(SchtasksError::Run(io::Error::other("not found"))), || false);

    assert!(matches!(outcome, Err(SchtasksError::Run(_))), "{outcome:?}");
}

#[test]
#[ignore = "needs an elevated terminal"]
fn the_task_can_be_created_and_removed() {
    let name = "Overlay Monitor test";
    let work = std::env::temp_dir().join(format!("overlay-monitor-startup-{}", std::process::id()));
    fs::create_dir_all(&work).expect("a work folder");
    let sid = platform_win32::account::current_user_sid().expect("a sid");
    let task = SchtasksStartup::new(name, std::env::current_exe().expect("an exe"), sid, work.clone());
    let exists = || {
        Command::new("schtasks.exe")
            .args(["/Query", "/TN", name])
            .output()
            .expect("schtasks runs")
            .status
            .success()
    };

    task.set(true).expect("registers");
    let present = exists();
    task.set(false).expect("removes");
    let after = exists();
    fs::remove_dir_all(&work).ok();

    assert!(present, "the task exists after set(true)");
    assert!(!after, "the task is gone after set(false)");
}
