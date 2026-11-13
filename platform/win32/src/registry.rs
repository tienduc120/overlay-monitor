//! Values under `HKEY_LOCAL_MACHINE`, read only.

use std::ffi::c_void;

use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::core::PCWSTR;

use crate::Win32Error;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// One `RegGetValueW` call; `data` is where the value goes and `bytes` its capacity in and its size out.
fn get_value(
    key: &[u16],
    value: &[u16],
    flags: REG_ROUTINE_FLAGS,
    data: Option<*mut c_void>,
    bytes: &mut u32,
) -> Result<(), Win32Error> {
    // SAFETY: `key` and `value` are NUL-terminated and outlive the call. `data`, when given, points at least `*bytes`
    // writable bytes (both callers pass a buffer sized for that), and `bytes` lives through the call.
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            flags,
            None,
            data,
            Some(std::ptr::from_mut(bytes)),
        )
    }
    .ok()
}

/// The string value `value` of `HKEY_LOCAL_MACHINE\{key}`.
///
/// # Errors
/// The key or value is missing, is not a string, or cannot be read.
pub fn read_string(key: &str, value: &str) -> Result<String, Win32Error> {
    let (key, value) = (wide(key), wide(value));
    let mut bytes = 0_u32;
    get_value(&key, &value, RRF_RT_REG_SZ, None, &mut bytes)?;
    let mut buffer = vec![0_u16; usize::try_from(bytes).unwrap_or(0).div_ceil(2)];
    let mut capacity = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    get_value(
        &key,
        &value,
        RRF_RT_REG_SZ,
        Some(buffer.as_mut_ptr().cast::<c_void>()),
        &mut capacity,
    )?;
    let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(buffer.get(..end).unwrap_or_default()))
}

/// The number value `value` of `HKEY_LOCAL_MACHINE\{key}`.
///
/// # Errors
/// The key or value is missing, is not a number, or cannot be read.
pub fn read_u32(key: &str, value: &str) -> Result<u32, Win32Error> {
    let (key, value) = (wide(key), wide(value));
    let mut number = 0_u32;
    let mut bytes = 4_u32;
    get_value(
        &key,
        &value,
        RRF_RT_REG_DWORD,
        Some(std::ptr::from_mut(&mut number).cast::<c_void>()),
        &mut bytes,
    )?;
    Ok(number)
}

#[cfg(test)]
mod tests;
