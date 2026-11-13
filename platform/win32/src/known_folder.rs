//! The per-user data folders, asked of the shell rather than read from the environment.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::GUID;

use crate::Win32Error;

/// `%APPDATA%`: settings that follow the user between machines.
///
/// # Errors
/// The shell could not resolve the folder.
pub fn roaming_app_data() -> Result<PathBuf, Win32Error> {
    known_folder(&FOLDERID_RoamingAppData)
}

/// `%LOCALAPPDATA%`: data that stays on this machine, such as logs.
///
/// # Errors
/// The shell could not resolve the folder.
pub fn local_app_data() -> Result<PathBuf, Win32Error> {
    known_folder(&FOLDERID_LocalAppData)
}

fn known_folder(id: &GUID) -> Result<PathBuf, Win32Error> {
    // SAFETY: `id` points to a valid GUID for the call; no token means the current user.
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }?;
    // SAFETY: on success `path` is a NUL-terminated string the shell allocated, valid until it is freed below.
    let folder = PathBuf::from(OsString::from_wide(unsafe { path.as_wide() }));
    // SAFETY: the shell requires the caller to free the string with `CoTaskMemFree`, once, after its last use.
    unsafe { CoTaskMemFree(Some(path.0.cast_const().cast())) };
    Ok(folder)
}

#[cfg(test)]
mod tests;
