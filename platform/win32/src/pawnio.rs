//! A client for the `PawnIO` driver: load a signed module, then call its functions.
//!
//! `PawnIO` binds a loaded module to the handle that loaded it, so every [`PawnModule`] opens its own handle.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_DATA, ERROR_INVALID_FUNCTION,
    ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED, ERROR_PATH_NOT_FOUND, GENERIC_READ, GENERIC_WRITE, HANDLE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::w;

use crate::Win32Error;

const PIO_DEVICE_TYPE: u32 = 41394;

const fn ctl_code(function: u32) -> u32 {
    // Method buffered and any access leave the two low flag fields zero.
    (PIO_DEVICE_TYPE << 16) | (function << 2)
}

const IOCTL_PIO_LOAD_BINARY: u32 = ctl_code(0x821);
const IOCTL_PIO_EXECUTE_FN: u32 = ctl_code(0x841);
/// Added in `PawnIO` 2.1.0; 2.0.0 answers `STATUS_NOT_IMPLEMENTED`.
const IOCTL_PIO_VERSION: u32 = ctl_code(0x861);

/// The driver reads a function name from a fixed 32-byte field.
const NAME_FIELD: usize = 32;

/// Why `PawnIO` could not be used.
#[derive(Debug, thiserror::Error)]
pub enum PawnError {
    /// The device does not exist: `PawnIO` is not installed.
    #[error("PawnIO is not installed")]
    NotInstalled,
    /// The caller is not elevated.
    #[error("PawnIO needs an elevated process")]
    AccessDenied,
    /// The module refused this CPU or chipset.
    #[error("the module does not support this machine")]
    NotSupported,
    /// A function name must fit the driver's 32-byte field with a terminating zero.
    #[error("function name is longer than 31 bytes")]
    NameTooLong,
    /// The driver has no version call, which only releases older than 2.1.0 lack.
    #[error("the driver does not report its version")]
    NoVersion,
    #[error("PawnIO call failed")]
    Io(#[source] Win32Error),
}

/// A module loaded into the driver, bound to its own device handle.
#[derive(Debug)]
pub struct PawnModule {
    handle: HANDLE,
}

// SAFETY: a device handle is an opaque kernel object id, not a pointer into this process; the driver serialises
// calls on it, and the handle is closed exactly once, in `Drop`.
unsafe impl Send for PawnModule {}

impl PawnModule {
    /// Opens the driver and loads `blob`, a signed module file such as `AMDFamily17.bin`.
    ///
    /// # Errors
    /// [`PawnError::NotInstalled`], [`PawnError::AccessDenied`], [`PawnError::NotSupported`] when the module rejects
    /// this machine, or [`PawnError::Io`].
    pub fn load(blob: &[u8]) -> Result<Self, PawnError> {
        let blob_len = len_u32(blob.len()).map_err(PawnError::Io)?;
        let module = Self { handle: open_device()? };
        // SAFETY: `blob` is a live slice of `blob_len` bytes, there is no output buffer, and the handle is open.
        unsafe {
            DeviceIoControl(
                module.handle,
                IOCTL_PIO_LOAD_BINARY,
                Some(blob.as_ptr().cast()),
                blob_len,
                None,
                0,
                None,
                None,
            )
        }
        .map_err(|e| map_error(e, true))?;
        Ok(module)
    }

    /// Calls `function` with `input` and returns at most `out_len` result words.
    ///
    /// # Errors
    /// [`PawnError::NameTooLong`], or [`PawnError::Io`] when the driver or the module refuses the call.
    pub fn execute(&self, function: &str, input: &[u64], out_len: usize) -> Result<Vec<u64>, PawnError> {
        let request = encode_execute(function, input)?;
        let request_len = len_u32(request.len()).map_err(PawnError::Io)?;
        let mut out = vec![0_u64; out_len];
        let out_bytes = len_u32(out.len() * size_of::<u64>()).map_err(PawnError::Io)?;
        let mut written = 0_u32;
        // SAFETY: both buffers are live locals of exactly the lengths passed, and `written` outlives the call.
        unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_PIO_EXECUTE_FN,
                Some(request.as_ptr().cast()),
                request_len,
                Some(out.as_mut_ptr().cast()),
                out_bytes,
                Some(&raw mut written),
                None,
            )
        }
        .map_err(|e| map_error(e, false))?;
        out.truncate(written as usize / size_of::<u64>());
        Ok(out)
    }
}

impl Drop for PawnModule {
    fn drop(&mut self) {
        // SAFETY: the handle was opened in `load` and is closed only here. A failed close leaves nothing to recover.
        unsafe { CloseHandle(self.handle) }.ok();
    }
}

/// The execute request: the function name zero-padded to 32 bytes, then each argument as a little-endian `u64`.
///
/// # Errors
/// [`PawnError::NameTooLong`] when the name leaves no room for its terminating zero.
pub fn encode_execute(function: &str, input: &[u64]) -> Result<Vec<u8>, PawnError> {
    if function.len() >= NAME_FIELD {
        return Err(PawnError::NameTooLong);
    }
    let mut buf = Vec::with_capacity(NAME_FIELD + size_of_val(input));
    buf.extend_from_slice(function.as_bytes());
    buf.resize(NAME_FIELD, 0);
    buf.extend(input.iter().flat_map(|word| word.to_le_bytes()));
    Ok(buf)
}

/// The installed driver's version, ordered so a floor can be compared against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PawnVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl std::fmt::Display for PawnVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Splits the driver's version word, `(major << 16) | (minor << 8) | patch`.
#[must_use]
pub const fn decode_version(raw: u32) -> PawnVersion {
    let [patch, minor, major, _] = raw.to_le_bytes();
    PawnVersion { major, minor, patch }
}

/// Asks the driver for its version; this reads nothing from hardware.
///
/// # Errors
/// [`PawnError::NotInstalled`], [`PawnError::AccessDenied`], [`PawnError::NoVersion`] for a driver older than 2.1.0,
/// or [`PawnError::Io`].
pub fn driver_version() -> Result<PawnVersion, PawnError> {
    // The module type owns the handle, so it closes on every path out.
    let device = PawnModule { handle: open_device()? };
    let mut raw = 0_u32;
    let mut written = 0_u32;
    // SAFETY: there is no input; the output is the live local `raw` of exactly four bytes, and `written` outlives the
    // call.
    let called = unsafe {
        DeviceIoControl(
            device.handle,
            IOCTL_PIO_VERSION,
            None,
            0,
            Some((&raw mut raw).cast()),
            4,
            Some(&raw mut written),
            None,
        )
    };
    called.map_err(|e| {
        let code = e.code();
        if code == ERROR_INVALID_FUNCTION.to_hresult() || code == ERROR_NOT_SUPPORTED.to_hresult() {
            PawnError::NoVersion
        } else {
            map_error(e, false)
        }
    })?;
    if written != 4 {
        return Err(PawnError::Io(ERROR_INVALID_DATA.to_hresult().into()));
    }
    Ok(decode_version(raw))
}

fn open_device() -> Result<HANDLE, PawnError> {
    // SAFETY: the path is a NUL-terminated literal and no security attributes or template handle are passed.
    unsafe {
        CreateFileW(
            w!(r"\\?\GLOBALROOT\Device\PawnIO"),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| map_error(e, false))
}

/// A length that does not fit the driver's 32-bit field must fail, never be truncated under an unsafe call.
fn len_u32(len: usize) -> Result<u32, Win32Error> {
    u32::try_from(len).map_err(|_too_long| ERROR_INVALID_PARAMETER.to_hresult().into())
}

fn map_error(error: Win32Error, loading_module: bool) -> PawnError {
    let code = error.code();
    if code == ERROR_FILE_NOT_FOUND.to_hresult() || code == ERROR_PATH_NOT_FOUND.to_hresult() {
        PawnError::NotInstalled
    } else if code == ERROR_ACCESS_DENIED.to_hresult() {
        PawnError::AccessDenied
    } else if loading_module && code == ERROR_NOT_SUPPORTED.to_hresult() {
        PawnError::NotSupported
    } else {
        PawnError::Io(error)
    }
}

#[cfg(test)]
mod tests;
