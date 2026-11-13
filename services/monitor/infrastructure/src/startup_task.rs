//! Start with Windows as a Task Scheduler task at logon with the highest privileges, so the elevated app starts
//! without a UAC prompt (ADR-0020). The task is registered by `schtasks.exe` from an XML definition.

use std::fs;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use monitor_application::{StartupTask, StartupTaskError};

/// The task's name in Task Scheduler.
pub const TASK_NAME: &str = "Overlay Monitor";

/// Keeps `schtasks.exe` from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Why `schtasks.exe` did not do what was asked.
#[derive(Debug, thiserror::Error)]
enum SchtasksError {
    #[error("write the task definition")]
    WriteDefinition(#[source] io::Error),
    #[error("run schtasks.exe")]
    Run(#[source] io::Error),
    #[error("schtasks.exe {action} failed ({status}): {output}")]
    Failed {
        action: &'static str,
        status: String,
        output: String,
    },
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The task definition: runs `exe` when `user_sid` signs in, with the highest privileges that user has, and is never
/// stopped (the default limits stop a task after three days and when the power goes to the battery).
pub fn task_xml(exe: &Path, user_sid: &str) -> String {
    let sid = escape(user_sid);
    let exe = escape(&exe.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Starts Overlay Monitor when you sign in.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command></Exec></Actions>
</Task>
"#
    )
}

/// `text` as a UTF-16LE file: the byte order mark `FF FE`, then the code units, as `schtasks.exe` expects.
pub fn utf16_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// Registers and removes the start-at-sign-in task by running `schtasks.exe`.
#[derive(Debug)]
pub struct SchtasksStartup {
    task_name: String,
    exe: PathBuf,
    user_sid: String,
    work_dir: PathBuf,
}

impl SchtasksStartup {
    /// `work_dir` holds the definition file for the moment `schtasks.exe` reads it.
    pub fn new(task_name: &str, exe: PathBuf, user_sid: String, work_dir: PathBuf) -> Self {
        Self {
            task_name: task_name.to_owned(),
            exe,
            user_sid,
            work_dir,
        }
    }

    fn schtasks(args: &[&str]) -> io::Result<Output> {
        Command::new("schtasks.exe")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    }

    fn checked(action: &'static str, args: &[&str]) -> Result<(), SchtasksError> {
        let output = Self::schtasks(args).map_err(SchtasksError::Run)?;
        if output.status.success() {
            return Ok(());
        }
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
        Err(SchtasksError::Failed {
            action,
            status: output.status.to_string(),
            output: format!("{} {}", text(&output.stdout), text(&output.stderr))
                .trim()
                .to_owned(),
        })
    }

    fn register(&self) -> Result<(), SchtasksError> {
        fs::create_dir_all(&self.work_dir).map_err(SchtasksError::WriteDefinition)?;
        let file = self.work_dir.join("startup-task.xml");
        let xml = task_xml(&self.exe, &self.user_sid);
        fs::write(&file, utf16_with_bom(&xml)).map_err(SchtasksError::WriteDefinition)?;
        let created = Self::checked(
            "/Create",
            &["/Create", "/TN", &self.task_name, "/XML", &file.to_string_lossy(), "/F"],
        );
        // The definition is only an input; a leftover copy is harmless, so a failed delete is not worth an error.
        fs::remove_file(&file).ok();
        created
    }

    fn remove(&self) -> Result<(), SchtasksError> {
        let query = Self::schtasks(&["/Query", "/TN", &self.task_name]).map_err(SchtasksError::Run)?;
        if !query.status.success() {
            return Ok(());
        }
        Self::checked("/Delete", &["/Delete", "/TN", &self.task_name, "/F"])
    }
}

impl StartupTask for SchtasksStartup {
    fn set(&self, on: bool) -> Result<(), StartupTaskError> {
        let outcome = if on { self.register() } else { self.remove() };
        outcome.map_err(|error| StartupTaskError(Box::new(error)))
    }
}

#[cfg(test)]
mod tests;
