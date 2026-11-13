//! Real-time event tracing (ETW): a named session that collects chosen providers' events, and a consumer that hands
//! each event to a closure.

use std::ffi::c_void;
use std::mem::offset_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

use windows::Win32::Foundation::{ERROR_INVALID_PARAMETER, ERROR_WMI_INSTANCE_NOT_FOUND};
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, ENABLE_TRACE_PARAMETERS, ENABLE_TRACE_PARAMETERS_VERSION_2,
    EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_FILTER_DESCRIPTOR, EVENT_FILTER_EVENT_ID, EVENT_FILTER_TYPE_EVENT_ID,
    EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_LOGFILEW_0, EVENT_TRACE_LOGFILEW_1,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2, MAX_EVENT_FILTER_EVENT_ID_COUNT, OpenTraceW,
    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE, ProcessTrace, StartTraceW,
    WNODE_FLAG_TRACED_GUID,
};
use windows::Win32::System::Performance::QueryPerformanceFrequency;
use windows::core::{GUID, HSTRING, PCWSTR, PWSTR};

use crate::Win32Error;

/// The longest session name and log file name ETW writes back, in UTF-16 units with the terminator.
const MAX_NAME: usize = 1024;
/// `WNODE_HEADER::ClientContext` for timestamps from the performance counter.
const QPC_CLOCK: u32 = 1;
/// Missing from the SDK headers (it is in `ntwmi.h`): `FlushTimer` counts milliseconds. Windows 8 and later. A Windows
/// that ignored it would read `FLUSH_MILLIS` as seconds: frames would then arrive in bursts, when a buffer fills or
/// every 10 s, still all counted, so readings would swing between `—` and several times the true rate.
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
    /// The only event ids the session lets in; empty lets in every event the level and keywords pass.
    pub event_ids: &'static [u16],
}

/// The most ids one event id filter may list.
const MAX_EVENT_IDS: usize = MAX_EVENT_FILTER_EVENT_ID_COUNT as usize;

/// `EVENT_FILTER_EVENT_ID` followed by room for the rest of the ids, so they lie right after the header's first one.
#[repr(C)]
struct EventIdFilter {
    header: EVENT_FILTER_EVENT_ID,
    more: [u16; MAX_EVENT_IDS - 1],
}

impl EventIdFilter {
    /// Lets in only `ids`. ETW accepts at most 64.
    fn new(ids: &[u16]) -> Result<Self, Win32Error> {
        let count = u16::try_from(ids.len())
            .ok()
            .filter(|&count| usize::from(count) <= MAX_EVENT_IDS)
            .ok_or_else(|| Win32Error::from(ERROR_INVALID_PARAMETER))?;
        let mut filter = Self {
            header: EVENT_FILTER_EVENT_ID {
                FilterIn: true,
                Reserved: 0,
                Count: count,
                Events: [0],
            },
            more: [0; MAX_EVENT_IDS - 1],
        };
        let slots = filter.header.Events.iter_mut().chain(filter.more.iter_mut());
        for (slot, id) in slots.zip(ids) {
            *slot = *id;
        }
        Ok(filter)
    }

    /// The header and the ids it lists, which is all `EVENT_FILTER_DESCRIPTOR::Size` may cover.
    fn size(&self) -> u32 {
        to_u32(offset_of!(EVENT_FILTER_EVENT_ID, Events) + usize::from(self.header.Count) * size_of::<u16>())
    }
}

/// The part of an event the consumer hands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventInfo {
    pub provider: u128,
    pub id: u16,
    pub process_id: u32,
    /// When the event was logged, in nanoseconds on the performance counter's clock.
    pub timestamp_ns: u64,
}

/// Performance counter ticks as nanoseconds; 0 for a negative time or no frequency, and `u64::MAX` past the range.
fn ticks_to_ns(ticks: i64, frequency: i64) -> u64 {
    let (Ok(ticks), Ok(frequency)) = (u128::try_from(ticks), u128::try_from(frequency)) else {
        return 0;
    };
    if frequency == 0 {
        return 0;
    }
    u64::try_from(ticks * 1_000_000_000 / frequency).unwrap_or(u64::MAX)
}

/// What `deliver` finds through the record's context.
struct Delivery<'a> {
    on_event: &'a mut (dyn FnMut(&EventInfo) + Send),
    /// Ticks per second of the event clock, stored once the trace is open and before any event arrives.
    frequency: &'a AtomicI64,
}

/// A running real-time session. Dropping it stops it.
#[derive(Debug)]
pub struct TraceSession {
    handle: CONTROLTRACE_HANDLE,
    shutdown: Arc<Shutdown>,
}

/// Reads one session's events.
#[derive(Debug)]
pub struct TraceConsumer {
    session_name: HSTRING,
    shutdown: Arc<Shutdown>,
}

/// What a session and its consumer share, so that stopping the session always ends the consumer's `run`.
#[derive(Debug)]
struct Shutdown {
    /// `stop` succeeded or is under way.
    stopped: AtomicBool,
    /// The system refused to stop the session, which may then never end, so the consumer's trace was closed instead.
    abandoned: AtomicBool,
    /// The consumer's open trace, or `INVALID_PROCESSTRACE_HANDLE`. Whoever swaps it out closes it.
    trace: AtomicU64,
}

impl Shutdown {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            stopped: AtomicBool::new(false),
            abandoned: AtomicBool::new(false),
            trace: AtomicU64::new(INVALID_PROCESSTRACE_HANDLE),
        })
    }

    fn is_over(&self) -> bool {
        self.stopped.load(Ordering::SeqCst) || self.abandoned.load(Ordering::SeqCst)
    }

    /// Ends the consumer without the session: closing its trace makes `ProcessTrace` return.
    fn abandon(&self) {
        self.abandoned.store(true, Ordering::SeqCst);
        // The stop that failed is the error worth reporting; a failed close adds nothing to it.
        self.close_trace().ok();
    }

    /// Closes the consumer's trace if it is open and nobody closed it yet.
    fn close_trace(&self) -> Result<(), Win32Error> {
        let trace = self.trace.swap(INVALID_PROCESSTRACE_HANDLE, Ordering::SeqCst);
        if trace == INVALID_PROCESSTRACE_HANDLE {
            return Ok(());
        }
        // SAFETY: the handle came from OpenTraceW, and the swap above made this its only close.
        unsafe { CloseTrace(PROCESSTRACE_HANDLE { Value: trace }) }.ok()
    }
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
        shutdown: Shutdown::new(),
    };
    for provider in providers {
        session.enable(provider)?;
    }
    let consumer = TraceConsumer {
        session_name,
        shutdown: Arc::clone(&session.shutdown),
    };
    Ok((session, consumer))
}

impl TraceSession {
    fn enable(&self, provider: &Provider) -> Result<(), Win32Error> {
        let guid = GUID::from_u128(provider.guid);
        let filter = EventIdFilter::new(provider.event_ids)?;
        let mut descriptor = EVENT_FILTER_DESCRIPTOR {
            // ETW reads the filter through this address during the call.
            Ptr: (&raw const filter).expose_provenance() as u64,
            Size: filter.size(),
            Type: EVENT_FILTER_TYPE_EVENT_ID,
        };
        let parameters = ENABLE_TRACE_PARAMETERS {
            Version: ENABLE_TRACE_PARAMETERS_VERSION_2,
            EnableFilterDesc: &raw mut descriptor,
            FilterDescCount: 1,
            ..ENABLE_TRACE_PARAMETERS::default()
        };
        let filtered = (!provider.event_ids.is_empty()).then_some(&raw const parameters);
        // SAFETY: the session is running; `guid`, `parameters`, the descriptor it points at, and the filter the
        // descriptor addresses are live locals that outlive the call.
        unsafe {
            EnableTraceEx2(
                self.handle,
                &raw const guid,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                provider.level,
                provider.match_any_keyword,
                0,
                0,
                filtered,
            )
        }
        .ok()
    }

    /// Stops the session, which ends its consumer's `run`, even one that has not started yet. Stopping a stopped
    /// session does nothing.
    ///
    /// # Errors
    /// The system refused to stop the session. The consumer's `run` still returns, because its trace is closed
    /// instead, and dropping the session tries to stop it again.
    pub fn stop(&self) -> Result<(), Win32Error> {
        if self.shutdown.stopped.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let stopped = stop(self.handle, None);
        if stopped.is_err() {
            self.shutdown.stopped.store(false, Ordering::SeqCst);
            self.shutdown.abandon();
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
    /// Hands every event of the session to `on_event`, on this thread, until the session stops. A session stopped
    /// before `run` makes it return at once.
    ///
    /// A panic in `on_event` aborts the process: it would have to unwind through ETW's C callback, which Rust forbids.
    ///
    /// # Errors
    /// The session's trace could not be opened or read.
    pub fn run(self, on_event: &mut (dyn FnMut(&EventInfo) + Send)) -> Result<(), Win32Error> {
        let shutdown = &self.shutdown;
        let frequency = AtomicI64::new(0);
        let mut delivery = Delivery {
            on_event,
            frequency: &frequency,
        };
        // `deliver` finds the closure through `delivery`, a local that outlives ProcessTrace below.
        let mut logfile = EVENT_TRACE_LOGFILEW {
            // OpenTraceW only reads the name.
            LoggerName: PWSTR(self.session_name.as_ptr().cast_mut()),
            Anonymous1: EVENT_TRACE_LOGFILEW_0 {
                ProcessTraceMode: PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD,
            },
            Anonymous2: EVENT_TRACE_LOGFILEW_1 {
                EventRecordCallback: Some(deliver),
            },
            Context: (&raw mut delivery).cast::<c_void>(),
            ..EVENT_TRACE_LOGFILEW::default()
        };
        // SAFETY: `logfile` is a live local naming a NUL-terminated session name that outlives the call.
        let trace = unsafe { OpenTraceW(&raw mut logfile) };
        if trace.Value == INVALID_PROCESSTRACE_HANDLE {
            let refused = Win32Error::from_thread();
            // A stop that came first leaves no session to open, which is the end the caller asked for.
            return if shutdown.is_over() { Ok(()) } else { Err(refused) };
        }
        // OpenTraceW wrote the clock's frequency into the header it was given; a zero there is asked of the system once.
        frequency.store(clock_frequency(logfile.LogfileHeader.PerfFreq), Ordering::SeqCst);
        shutdown.trace.store(trace.Value, Ordering::SeqCst);
        // `abandon` stores its flag before it swaps the trace out, and this stored the trace before it reads the flag,
        // so one of the two sees the other and the trace gets closed.
        if shutdown.abandoned.load(Ordering::SeqCst) {
            // As below: the failed stop already reported why, and a failed close adds nothing to it.
            shutdown.close_trace().ok();
            return Ok(());
        }
        // SAFETY: the trace is open, or was just closed by `abandon`, in which case ProcessTrace returns at once. Every
        // event reaches `deliver` on this thread before ProcessTrace returns, and the delivery its context points at is
        // borrowed for this whole function.
        let processed = unsafe { ProcessTrace(&[trace], None, None) }.ok();
        let closed = shutdown.close_trace();
        if shutdown.is_over() {
            // The session ended because the caller stopped it, before or during the read; whatever ProcessTrace says
            // about a session that is gone (a stop that came first makes it "not found") is that same end.
            return Ok(());
        }
        processed?;
        closed
    }
}

/// Hands one event to the consumer's closure.
///
/// # Safety
/// Only `ProcessTrace` calls this, with a record whose `UserContext` is the `Context` that `TraceConsumer::run` gave
/// `OpenTraceW`: a pointer to its `Delivery`.
unsafe extern "system" fn deliver(record: *mut EVENT_RECORD) {
    // SAFETY: ProcessTrace passes a record that is valid for the duration of this call, or null.
    let Some(record) = (unsafe { record.as_ref() }) else {
        return;
    };
    let delivery = record.UserContext.cast::<Delivery<'_>>();
    // SAFETY: per this function's contract the context points at `run`'s delivery, which `run` does not touch while
    // ProcessTrace runs, so this is the only reference to it.
    let Some(delivery) = (unsafe { delivery.as_mut() }) else {
        return;
    };
    let header = &record.EventHeader;
    (delivery.on_event)(&EventInfo {
        provider: header.ProviderId.to_u128(),
        id: header.EventDescriptor.Id,
        process_id: header.ProcessId,
        timestamp_ns: ticks_to_ns(header.TimeStamp, delivery.frequency.load(Ordering::SeqCst)),
    });
}

/// The trace's tick rate: what `OpenTraceW` wrote, else the performance counter's own.
fn clock_frequency(reported: i64) -> i64 {
    if reported > 0 {
        return reported;
    }
    let mut frequency = 0;
    // SAFETY: the pointer comes from a live, exclusively borrowed local.
    unsafe { QueryPerformanceFrequency(&raw mut frequency) }.ok();
    frequency
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
