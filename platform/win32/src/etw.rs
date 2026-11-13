//! Real-time event tracing (ETW): a named session that collects chosen providers' events, and a consumer that hands
//! each event to a closure.

use std::ffi::c_void;
use std::mem::offset_of;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::ERROR_WMI_INSTANCE_NOT_FOUND;
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_RECORD,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_LOGFILEW_0, EVENT_TRACE_LOGFILEW_1,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2, OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_REAL_TIME, ProcessTrace, StartTraceW, WNODE_FLAG_TRACED_GUID,
};
use windows::core::{GUID, HSTRING, PCWSTR, PWSTR};

use crate::Win32Error;

/// The longest session name and log file name ETW writes back, in UTF-16 units with the terminator.
const MAX_NAME: usize = 1024;
/// `WNODE_HEADER::ClientContext` for timestamps from the performance counter.
const QPC_CLOCK: u32 = 1;
/// Missing from the SDK headers (it is in `ntwmi.h`): `FlushTimer` counts milliseconds. Windows 8 and later.
const EVENT_TRACE_USE_MS_FLUSH_TIMER: u32 = 0x10;
/// Buffers reach the consumer at least this often, so a reading taken once a second sees that second's events, not a
/// burst whenever a buffer happens to fill (a steady 240 Hz app read 238 to 245 once a second).
const FLUSH_MILLIS: u32 = 10;
/// What `OpenTraceW` returns when it fails.
const INVALID_PROCESSTRACE_HANDLE: u64 = u64::MAX;

/// A provider to enable, and the level and keywords of the events wanted from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    pub guid: u128,
    pub level: u8,
    pub match_any_keyword: u64,
}

/// The part of an event the consumer hands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventInfo {
    pub provider: u128,
    pub id: u16,
    pub process_id: u32,
}

/// A running real-time session. Dropping it stops it.
#[derive(Debug)]
pub struct TraceSession {
    handle: CONTROLTRACE_HANDLE,
    stopped: AtomicBool,
}

/// Reads one session's events.
#[derive(Debug)]
pub struct TraceConsumer {
    session_name: HSTRING,
}

/// Starts the real-time session `name` with `providers` enabled, first stopping a session of that name that a
/// crashed run left behind (Windows keeps it alive after its process dies).
///
/// # Errors
/// Starting a session or enabling a provider was refused: access denied for an account that is neither elevated nor
/// in Performance Log Users.
pub fn start_session(name: &str, providers: &[Provider]) -> Result<(TraceSession, TraceConsumer), Win32Error> {
    let session_name = HSTRING::from(name);
    stop(CONTROLTRACE_HANDLE::default(), Some(&session_name))?;

    let mut properties = Properties::new();
    properties.header.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    properties.header.Wnode.ClientContext = QPC_CLOCK;
    properties.header.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_USE_MS_FLUSH_TIMER;
    properties.header.FlushTimer = FLUSH_MILLIS;
    let mut handle = CONTROLTRACE_HANDLE::default();
    // SAFETY: `handle` is a live local, `session_name` is NUL-terminated, and `properties` is as large as its header
    // says, with room for the session name StartTraceW copies in.
    unsafe { StartTraceW(&raw mut handle, &session_name, properties.as_mut_ptr()) }.ok()?;

    // From here dropping the session stops it, so a refused provider leaves nothing running.
    let session = TraceSession {
        handle,
        stopped: AtomicBool::new(false),
    };
    for provider in providers {
        session.enable(provider)?;
    }
    Ok((session, TraceConsumer { session_name }))
}

impl TraceSession {
    fn enable(&self, provider: &Provider) -> Result<(), Win32Error> {
        let guid = GUID::from_u128(provider.guid);
        // SAFETY: the session is running, `guid` is a live local, and no filter parameters are passed.
        unsafe {
            EnableTraceEx2(
                self.handle,
                &raw const guid,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                provider.level,
                provider.match_any_keyword,
                0,
                0,
                None,
            )
        }
        .ok()
    }

    /// Stops the session, which ends its consumer's `run`. Stopping a stopped session does nothing.
    ///
    /// # Errors
    /// The system refused to stop the session.
    pub fn stop(&self) -> Result<(), Win32Error> {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let stopped = stop(self.handle, None);
        if stopped.is_err() {
            self.stopped.store(false, Ordering::Release);
        }
        stopped
    }
}

impl Drop for TraceSession {
    fn drop(&mut self) {
        // A session that will not stop leaves nothing to do here; the next start stops it by name.
        self.stop().ok();
    }
}

impl TraceConsumer {
    /// Hands every event of the session to `on_event`, on this thread, until the session stops.
    ///
    /// # Errors
    /// The session could not be opened (it already stopped) or its events could not be read.
    pub fn run(self, mut on_event: &mut (dyn FnMut(&EventInfo) + Send)) -> Result<(), Win32Error> {
        // `deliver` finds the closure through `on_event`, a local that outlives ProcessTrace below.
        let mut logfile = EVENT_TRACE_LOGFILEW {
            // OpenTraceW only reads the name.
            LoggerName: PWSTR(self.session_name.as_ptr().cast_mut()),
            Anonymous1: EVENT_TRACE_LOGFILEW_0 {
                ProcessTraceMode: PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD,
            },
            Anonymous2: EVENT_TRACE_LOGFILEW_1 {
                EventRecordCallback: Some(deliver),
            },
            Context: (&raw mut on_event).cast::<c_void>(),
            ..EVENT_TRACE_LOGFILEW::default()
        };
        // SAFETY: `logfile` is a live local naming a NUL-terminated session name that outlives the call.
        let trace = unsafe { OpenTraceW(&raw mut logfile) };
        if trace.Value == INVALID_PROCESSTRACE_HANDLE {
            return Err(Win32Error::from_thread());
        }
        // SAFETY: the trace is open; every event reaches `deliver` on this thread before ProcessTrace returns, and the
        // closure its context points at is borrowed for this whole function.
        let processed = unsafe { ProcessTrace(&[trace], None, None) };
        // SAFETY: the trace was opened above and is closed exactly once, here.
        let closed = unsafe { CloseTrace(trace) };
        processed.ok()?;
        closed.ok()
    }
}

/// Hands one event to the consumer's closure.
///
/// # Safety
/// Only `ProcessTrace` calls this, with a record whose `UserContext` is the `Context` that `TraceConsumer::run` gave
/// `OpenTraceW`: a pointer to its `&mut` closure.
unsafe extern "system" fn deliver(record: *mut EVENT_RECORD) {
    // SAFETY: ProcessTrace passes a record that is valid for the duration of this call, or null.
    let Some(record) = (unsafe { record.as_ref() }) else {
        return;
    };
    let on_event = record.UserContext.cast::<&mut (dyn FnMut(&EventInfo) + Send)>();
    // SAFETY: per this function's contract the context points at `run`'s closure, which `run` does not touch while
    // ProcessTrace runs, so this is the only reference to it.
    let Some(on_event) = (unsafe { on_event.as_mut() }) else {
        return;
    };
    let header = &record.EventHeader;
    on_event(&EventInfo {
        provider: header.ProviderId.to_u128(),
        id: header.EventDescriptor.Id,
        process_id: header.ProcessId,
    });
}

/// Stops a session by its handle, or by `name` when the handle is null. A session that does not exist counts as
/// stopped.
fn stop(handle: CONTROLTRACE_HANDLE, name: Option<&HSTRING>) -> Result<(), Win32Error> {
    let mut properties = Properties::new();
    // Stopping also writes back the log file name, so it needs room too.
    properties.header.LogFileNameOffset = to_u32(offset_of!(Properties, log_file_name));
    let name = name.map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr()));
    // SAFETY: `name` is null or a NUL-terminated string borrowed for the call, and `properties` is as large as its
    // header says, with room for both names ControlTraceW writes back.
    let status = unsafe { ControlTraceW(handle, name, properties.as_mut_ptr(), EVENT_TRACE_CONTROL_STOP) };
    if status == ERROR_WMI_INSTANCE_NOT_FOUND {
        return Ok(());
    }
    status.ok()
}

/// `EVENT_TRACE_PROPERTIES` followed by the room the calls need for the names they write back.
#[repr(C)]
struct Properties {
    header: EVENT_TRACE_PROPERTIES,
    session_name: [u16; MAX_NAME],
    log_file_name: [u16; MAX_NAME],
}

impl Properties {
    fn new() -> Self {
        let mut properties = Self {
            header: EVENT_TRACE_PROPERTIES::default(),
            session_name: [0; MAX_NAME],
            log_file_name: [0; MAX_NAME],
        };
        properties.header.Wnode.BufferSize = to_u32(size_of::<Self>());
        properties.header.LoggerNameOffset = to_u32(offset_of!(Self, session_name));
        properties
    }

    /// The whole buffer, not just the header, so a call may write past the header into the names.
    fn as_mut_ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        (&raw mut *self).cast()
    }
}

/// The properties are a few KiB, so their size and offsets always fit.
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
