//! The text of the exe's resource script: its icon, and the version info Task Manager's Processes tab and the file's
//! Details tab show. `build.rs` includes this file to write the script; the library includes it for its tests only, as
//! a build script has none of its own.

/// The script for `embed-resource`: the icon at `icon_path` (forward slashes) and the version info for `version`,
/// which is the package's, so it is never typed twice.
pub(crate) fn resource_script(version: &str, icon_path: &str) -> String {
    let [major, minor, patch] = numbers(version);
    let numeric = format!("{major},{minor},{patch},0");
    format!(
        "1 ICON \"{icon_path}\"\n\
         \n\
         1 VERSIONINFO\n\
         FILEVERSION {numeric}\n\
         PRODUCTVERSION {numeric}\n\
         FILEFLAGSMASK 0x3fL\n\
         FILEFLAGS 0x0L\n\
         FILEOS 0x40004L\n\
         FILETYPE 0x1L\n\
         FILESUBTYPE 0x0L\n\
         BEGIN\n\
         \x20   BLOCK \"StringFileInfo\"\n\
         \x20   BEGIN\n\
         \x20       BLOCK \"040904B0\"\n\
         \x20       BEGIN\n\
         \x20           VALUE \"FileDescription\", \"Overlay Monitor\"\n\
         \x20           VALUE \"FileVersion\", \"{version}\"\n\
         \x20           VALUE \"InternalName\", \"overlay-monitor.exe\"\n\
         \x20           VALUE \"LegalCopyright\", \"Copyright (c) 2026 Luu Tien Duc\"\n\
         \x20           VALUE \"OriginalFilename\", \"overlay-monitor.exe\"\n\
         \x20           VALUE \"ProductName\", \"Overlay Monitor\"\n\
         \x20           VALUE \"ProductVersion\", \"{version}\"\n\
         \x20       END\n\
         \x20   END\n\
         \x20   BLOCK \"VarFileInfo\"\n\
         \x20   BEGIN\n\
         \x20       VALUE \"Translation\", 0x409, 1200\n\
         \x20   END\n\
         END\n"
    )
}

/// The three leading numbers of a version, each 0 when it is not one, which is what a pre-release suffix leaves out.
fn numbers(version: &str) -> [u16; 3] {
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.').map(|part| part.parse().unwrap_or(0));
    [(); 3].map(|()| parts.next().unwrap_or(0))
}

#[cfg(test)]
#[path = "version_info/tests.rs"]
mod tests;
