//! The composition root: the one place that knows which adapter backs each sensor. It installs the subscriber,
//! opens the windows, and runs the sampler and the frame counter beside the UI loop.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use monitor_application::{
    DriverStatus, GpuPair, GpuSensors, OverlayFrame, Sampler, SamplerControl, Sensor, Sensors, Settings, SettingsStore,
    StartupTask, StartupTaskError, load_settings,
};
use monitor_infrastructure::{
    AdapterSensors, CpuLoad, Ddr5Temperature, FrameCounter, FrameRate, MemoryLoad, PRESENT_PROVIDERS, PawnIoProbe,
    RollingFile, SchtasksStartup, TASK_NAME, TomlSettingsStore, UnavailableSensor, WindowsSystemInfo, cpu_temperature,
    is_present, memory_kind, memory_text, memory_types, reopening, settings_path,
};
use monitor_interfaces::{OverlayView, Ui, UiParts};
use platform_clock::{Clock, SystemClock};
use platform_win32::Win32Error;
use platform_win32::account::current_user_sid;
use platform_win32::clipboard::set_text;
use platform_win32::cpu::cpu_identity;
use platform_win32::dpi::{enable_per_monitor_awareness, small_icon_size};
use platform_win32::etw::{SessionStopper, TraceConsumer, TraceSession, start_session, stop_session};
use platform_win32::firmware::smbios_table;
use platform_win32::gpu::{GpuAdapter, gpu_adapters};
use platform_win32::icon::Icon;
use platform_win32::instance::SingleInstance;
use platform_win32::known_folder::{local_app_data, roaming_app_data};
use platform_win32::memory::trim_working_set;
use platform_win32::overlay::OverlayWindow;
use platform_win32::shell::show_error;
use platform_win32::tray::TrayIcon;
use platform_win32::ui::{MessageWindow, run_message_loop};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Held while a copy runs, so a second copy in this session exits at once.
const INSTANCE_NAME: &str = r"Local\OverlayMonitor";
const TRACE_SESSION: &str = "OverlayMonitor";
/// How long a stopped trace waits before it is started again.
const RESTART_DELAY: Duration = Duration::from_secs(5);
const TOOLTIP: &str = "Overlay Monitor";
/// How long after start the one trim of the working set waits, so start-up allocations are done by then.
const START_UP_TRIM_DELAY: Duration = Duration::from_secs(5);
/// The owner's icon, in the file's several sizes.
const OWNER_ICON: &[u8] = include_bytes!("../assets/icon.ico");
const LOG_LIMIT_BYTES: u64 = 1024 * 1024;
const LOG_KEEP: usize = 3;
const FPS_NEEDS_ADMIN: &str = "frame counting needs admin or the Performance Log Users group";

/// Why the overlay could not open.
#[derive(Debug, thiserror::Error)]
enum OpenError {
    #[error("create the message window")]
    MessageWindow(#[source] Win32Error),
    #[error("create the overlay window")]
    OverlayWindow(#[source] Win32Error),
    #[error("make the tray icon image")]
    TrayIcon(#[source] Win32Error),
}

/// Where `cargo xtask modules` puts the `PawnIO` modules: `modules` next to the exe.
pub fn modules_dir(exe: &Path) -> PathBuf {
    exe.with_file_name("modules")
}

/// Every sensor the overlay reads, around the GPU pair already opened. One that cannot open on this machine reads as
/// unavailable, so its line shows `—` and the others still work.
pub fn build_sensors(modules: &Path, gpu: GpuPair, frames: Arc<FrameCounter>) -> Sensors {
    let cpu = cpu_identity();
    let cpu_vendor = cpu.vendor;
    Sensors {
        fps: Box::new(FrameRate::new(frames, std::process::id())),
        cpu_load: Box::new(CpuLoad::new()),
        cpu_temp: reopening(|| PawnIoProbe.pawnio(), {
            let modules = modules.to_owned();
            move || cpu_temperature(&modules, &cpu)
        }),
        gpu_load: gpu.load,
        gpu_temp: gpu.temp,
        memory_load: Box::new(MemoryLoad::new()),
        memory_temp: reopening(|| PawnIoProbe.pawnio(), {
            let modules = modules.to_owned();
            move || Ddr5Temperature::open(&modules, &cpu_vendor)
        }),
    }
}

/// The memory type byte of each module SMBIOS lists; a table that cannot be read counts as none.
fn detect_memory() -> Vec<u8> {
    let types = smbios_table()
        .inspect_err(|error| tracing::warn!(?error, "the SMBIOS table could not be read"))
        .map(|table| memory_types(&table))
        .unwrap_or_default();
    tracing::info!(memory = %memory_text(&types), "installed memory");
    types
}

fn unavailable(reason: &str) -> Box<dyn Sensor> {
    Box::new(UnavailableSensor(reason.to_owned()))
}

/// Runs the overlay until the user exits it or Windows asks it to close.
pub fn run() -> ExitCode {
    let log_dir = install_subscriber();
    if let Err(error) = enable_per_monitor_awareness() {
        // Something set it first; the overlay still runs, only sized for whatever awareness that was.
        tracing::warn!(?error, "per-monitor DPI awareness was already set");
    }
    let _instance = match SingleInstance::acquire(INSTANCE_NAME) {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            tracing::info!("another copy is running");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            return fail_to_start(&error, "claim the single-instance name", log_dir.as_deref());
        }
    };
    let store: Box<dyn SettingsStore> = Box::new(TomlSettingsStore::new(settings_file()));
    let memory = detect_memory();
    let settings = load_settings(store.as_ref(), memory_kind(&memory));
    if settings.start_with_windows
        && let Ok(task) =
            own_startup_task().inspect_err(|reason| tracing::warn!(reason, "the startup task is not refreshed"))
    {
        refresh_startup_task(&settings, &task);
    }
    let adapters = listed_adapters();
    let machine = Machine {
        gpu_names: adapters.iter().map(|adapter| adapter.name.clone()).collect(),
        system: WindowsSystemInfo::new(adapters.clone(), cpu_identity(), memory),
    };
    let mut gpus = AdapterSensors::new(adapters);
    let gpu = gpus.open(settings.gpu.as_deref());
    let trace = start_frame_trace();
    let counter = FrameCounter::new();
    let mut sensors = build_sensors(&own_modules_dir(), gpu, Arc::clone(&counter));
    if trace.is_none() {
        sensors.fps = unavailable(FPS_NEEDS_ADMIN);
    }
    let sampler = Sampler::new(sensors, Box::new(gpus), SystemClock, &settings);
    match show_overlay(sampler, settings, store, machine, trace, &counter) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail_to_start(&error, "open the overlay", log_dir.as_deref()),
    }
}

/// The log folder: `logs` in the app's folder under `%LOCALAPPDATA%`, or `None` when Windows cannot name it.
fn log_dir() -> Option<PathBuf> {
    local_app_data()
        .ok()
        .map(|folder| folder.join("OverlayMonitor").join("logs"))
}

/// Logs to stderr and to the rolling file. Returns the folder the file is in, or `None` when it could not be opened,
/// which leaves stderr alone and says so.
fn install_subscriber() -> Option<PathBuf> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    let dir = log_dir();
    let opened = dir
        .as_deref()
        .map(|dir| RollingFile::open(dir, LOG_LIMIT_BYTES, LOG_KEEP));
    let (file, failure) = match opened {
        Some(Ok(file)) => (Some(file), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let file_layer = file.map(|file| fmt::layer().with_writer(Mutex::new(file)).with_ansi(false));
    #[expect(clippy::disallowed_methods, reason = "the composition root installs the subscriber")]
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(file_layer)
        .init();
    match (&failure, &dir) {
        (Some(error), _) => tracing::warn!(?error, "the log file could not be opened, so logging is to stderr only"),
        (None, None) => tracing::warn!("the local app data folder is unknown, so logging is to stderr only"),
        (None, Some(_)) => {}
    }
    dir.filter(|_| failure.is_none())
}

/// What the message box says when the app could not start: the error and each cause under it, and where the log is.
pub fn startup_failure_text(error: &(dyn Error + 'static), log_dir: Option<&Path>) -> String {
    let mut texts = vec![error.to_string()];
    let mut cause = error.source();
    while let Some(next) = cause {
        texts.push(next.to_string());
        cause = next.source();
    }
    let mut text = format!("Overlay Monitor could not start: {}", texts.join(": "));
    if let Some(dir) = log_dir {
        text = format!("{text}\r\n\r\nThe log is in {}.", dir.display());
    }
    text
}

/// Logs a startup failure and tells the user, as the release exe has no console to print to.
fn fail_to_start(error: &(dyn Error + 'static), what: &str, log_dir: Option<&Path>) -> ExitCode {
    let text = startup_failure_text(error, log_dir);
    tracing::error!(what, ?error, "startup failed");
    show_error("Overlay Monitor", &text);
    ExitCode::FAILURE
}

/// Registers the start at sign-in again when the setting is on, so the task follows an exe that moved. A refusal is
/// logged; the app runs either way.
pub fn refresh_startup_task(settings: &Settings, task: &dyn StartupTask) {
    if settings.start_with_windows
        && let Err(error) = task.set(true)
    {
        tracing::warn!(?error, "register the start with Windows again");
    }
}

/// The task for this exe and this user, or why either cannot be named.
fn own_startup_task() -> Result<SchtasksStartup, String> {
    let exe = std::env::current_exe().map_err(|error| format!("the exe is unknown: {error}"))?;
    let sid = current_user_sid().map_err(|error| format!("the user is unknown: {error}"))?;
    let work_dir = settings_file()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| String::from("the settings folder is unknown"))?;
    Ok(SchtasksStartup::new(TASK_NAME, exe, sid, work_dir))
}

/// The startup task when this exe or user cannot be named: every change answers why, so the settings window shows it.
pub struct UnavailableStartup(pub String);

impl StartupTask for UnavailableStartup {
    fn set(&self, _on: bool) -> Result<(), StartupTaskError> {
        Err(StartupTaskError(self.0.clone().into()))
    }
}

fn startup_task() -> Box<dyn StartupTask> {
    match own_startup_task() {
        Ok(task) => Box::new(task),
        Err(reason) => {
            tracing::warn!(reason, "start with Windows cannot be changed");
            Box::new(UnavailableStartup(reason))
        }
    }
}

/// Every GPU adapter Windows lists, or none when the list cannot be had.
fn listed_adapters() -> Vec<GpuAdapter> {
    gpu_adapters()
        .inspect_err(|error| tracing::warn!(?error, "list the GPU adapters"))
        .unwrap_or_default()
}

/// The frame trace, or `None` when this account may not start one.
fn start_frame_trace() -> Option<(TraceSession, TraceConsumer)> {
    start_session(TRACE_SESSION, PRESENT_PROVIDERS)
        .inspect_err(|error| tracing::warn!(?error, "{FPS_NEEDS_ADMIN}"))
        .ok()
}

fn own_modules_dir() -> PathBuf {
    match std::env::current_exe() {
        Ok(exe) => modules_dir(&exe),
        Err(error) => {
            tracing::warn!(
                ?error,
                "the exe's path is unknown, so the modules are looked for in the working directory"
            );
            PathBuf::from("modules")
        }
    }
}

/// `settings.toml` in the app's folder under `%APPDATA%`, or next to the exe when Windows cannot name that folder.
fn settings_file() -> PathBuf {
    match roaming_app_data() {
        Ok(folder) => settings_path(&folder),
        Err(error) => {
            tracing::error!(
                ?error,
                "the app data folder is unknown, so the settings are kept next to the exe"
            );
            std::env::current_exe().map_or_else(
                |_| PathBuf::from("settings.toml"),
                |exe| exe.with_file_name("settings.toml"),
            )
        }
    }
}

/// What the UI is told of this machine: the adapters the settings offer, and the source of the diagnostics.
struct Machine {
    gpu_names: Vec<String>,
    system: WindowsSystemInfo,
}

/// Opens the overlay and the tray icon, runs the sampler and the frame counter beside the UI loop until it ends, then
/// stops them in order.
fn show_overlay(
    mut sampler: Sampler<SystemClock>,
    settings: Settings,
    store: Box<dyn SettingsStore>,
    machine: Machine,
    trace: Option<(TraceSession, TraceConsumer)>,
    counter: &FrameCounter,
) -> Result<(), OpenError> {
    let window = MessageWindow::create().map_err(OpenError::MessageWindow)?;
    let overlay = OverlayWindow::create(window.sink()).map_err(OpenError::OverlayWindow)?;
    let icon = tray_icon_image().map_err(OpenError::TrayIcon)?;
    let tray = TrayIcon::create(&window, TOOLTIP, &icon);
    // A session that ends kills the process soon after, and the session would outlive it; the loop may be too late.
    // A failure here cannot be reported, as the process is going.
    window.on_session_end(|| {
        stop_session(TRACE_SESSION).ok();
    });
    let (frames, received) = mpsc::channel();
    let view = OverlayView::new(overlay);
    let (control, controls) = mpsc::channel();
    let mut ui = Ui::new(
        &window,
        UiParts {
            view,
            tray,
            frames: received,
            sampler: control,
            settings,
            store,
            gpu_names: machine.gpu_names,
            startup: startup_task(),
            driver: Box::new(PawnIoProbe),
            system: Box::new(machine.system),
            clipboard: Box::new(set_text),
        },
    );
    let waker = window.waker();
    let running = FrameTrace::new();
    let (stop_sender, stop) = mpsc::channel::<()>();
    std::thread::scope(|scope| {
        scope.spawn(move || sample_until_stopped(&mut sampler, &frames, || waker.wake(), &controls));
        if let Some(first) = trace {
            let running = &running;
            scope.spawn(move || count_frames_until_stopped(first, TRACE_SESSION, counter, running, &stop));
        }
        let mut start_up_trim = StartupTrim::new(Instant::now(), START_UP_TRIM_DELAY);
        run_message_loop(&window, &mut |event| {
            if start_up_trim.is_due(Instant::now()) {
                trim_working_set()
                    .inspect_err(|error| tracing::warn!(?error, "the working set was not trimmed"))
                    .ok();
            }
            ui.handle(&window, event)
        });
        // The scope joins both threads next: the UI holds the control sender, so dropping it lets the sampler end.
        // Stopping the running trace through its stopper ends the frame thread's consumer even when the system refuses
        // the stop, and keeps a session started meanwhile from running; the dropped sender ends a wait to restart.
        drop(ui);
        if let Err(error) = running.stop() {
            tracing::warn!(?error, "the frame trace would not stop, so its reader was abandoned");
        }
        drop(stop_sender);
    });
    // The tray icon, which `ui` held, went before the window the shell knows it by; the image it shows drops last.
    drop(window);
    Ok(())
}

/// The one trim of the working set that follows start-up: due once, at the first look at or after its deadline.
struct StartupTrim {
    deadline: Instant,
    done: bool,
}

impl StartupTrim {
    fn new(start: Instant, delay: Duration) -> Self {
        Self {
            deadline: start + delay,
            done: false,
        }
    }

    fn is_due(&mut self, now: Instant) -> bool {
        let due = !self.done && now >= self.deadline;
        self.done |= due;
        due
    }
}

/// The owner's icon at the tray's size, or the stock one when it cannot be made.
fn tray_icon_image() -> Result<Icon, Win32Error> {
    Icon::from_ico(OWNER_ICON, small_icon_size())
        .inspect_err(|error| tracing::warn!(?error, "use the stock tray icon, as the owner's could not be made"))
        .or_else(|_| Icon::stock())
}

/// What the sampler does after a wait.
enum Round {
    /// An interval passed since the last read: read every sensor.
    Sample,
    /// New settings came: draw the readings as they stand in them.
    Redraw,
    Stop,
}

/// Sends a frame and wakes the UI to draw it once an interval, and a redrawn one at once after each `Apply`, until the
/// control sender is gone. A preview does not put off the next read, so a run of them keeps the readings fresh.
fn sample_until_stopped<C: Clock>(
    sampler: &mut Sampler<C>,
    frames: &Sender<OverlayFrame>,
    wake: impl Fn(),
    control: &Receiver<SamplerControl>,
) {
    let mut sampled_at = Instant::now();
    let mut frame = sampler.sample();
    loop {
        if frames.send(frame).is_err() {
            // Nobody is left to draw a frame.
            return;
        }
        wake();
        frame = match wait_for_next_round(sampler, control, sampled_at) {
            Round::Sample => {
                sampled_at = Instant::now();
                sampler.sample()
            }
            Round::Redraw => sampler.render(),
            Round::Stop => return,
        };
    }
}

/// Waits until an interval after `sampled_at` (the interval the settings name now), or until settings arrive. A
/// report is answered from the readings as they stand and does not end the wait: sampling again for it would query the
/// hardware once more.
fn wait_for_next_round<C: Clock>(
    sampler: &mut Sampler<C>,
    control: &Receiver<SamplerControl>,
    sampled_at: Instant,
) -> Round {
    loop {
        let due = sampled_at
            .checked_add(sampler.interval())
            .map_or(Duration::ZERO, |due| due.saturating_duration_since(Instant::now()));
        match control.recv_timeout(due) {
            Err(RecvTimeoutError::Timeout) => return Round::Sample,
            Ok(SamplerControl::Apply(settings)) => {
                sampler.apply(&settings);
                return Round::Redraw;
            }
            Ok(SamplerControl::Report(reply)) => {
                // The UI stopped waiting when its reply channel is gone, which is not worth a log line.
                reply.send(sampler.report()).ok();
            }
            Err(RecvTimeoutError::Disconnected) => return Round::Stop,
        }
    }
}

/// Ends a running frame trace from the exit path; the app's is the session's [`SessionStopper`].
trait StopTrace {
    /// # Errors
    /// The system refused to stop the session. Its reader was abandoned instead, so its `run` still returns.
    fn stop(&self) -> Result<(), Win32Error>;
}

impl StopTrace for SessionStopper {
    fn stop(&self) -> Result<(), Win32Error> {
        Self::stop(self)
    }
}

/// The frame trace as both the thread that owns its sessions and the exit path see it. The exit stops the running
/// session through its stopper, which ends the thread's `run` even when the system refuses the stop, so the scope always
/// joins; a session the thread starts after that is never run.
struct FrameTrace<S> {
    state: Mutex<TraceState<S>>,
}

enum TraceState<S> {
    Idle,
    Running(S),
    Stopping,
}

impl<S: StopTrace> FrameTrace<S> {
    const fn new() -> Self {
        Self {
            state: Mutex::new(TraceState::Idle),
        }
    }

    /// Only whole values are stored under the lock, so a panic elsewhere while holding it does not spoil them.
    fn state(&self) -> MutexGuard<'_, TraceState<S>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Called by the thread before it runs a session's reader: `false` once the app is stopping, when it must not.
    fn begin(&self, stopper: S) -> bool {
        let mut state = self.state();
        if matches!(*state, TraceState::Stopping) {
            return false;
        }
        *state = TraceState::Running(stopper);
        true
    }

    /// Called by the thread once the reader returned, so the exit has nothing left to stop.
    fn end(&self) {
        let mut state = self.state();
        if matches!(*state, TraceState::Running(_)) {
            *state = TraceState::Idle;
        }
    }

    fn is_stopping(&self) -> bool {
        matches!(*self.state(), TraceState::Stopping)
    }

    /// Called at exit: stops the session that runs, if one does, and keeps any later one from running. The stop is made
    /// outside the lock, so the thread is never kept waiting on it.
    ///
    /// # Errors
    /// The running session would not stop; its reader was abandoned, so the thread still ends.
    fn stop(&self) -> Result<(), Win32Error> {
        let before = std::mem::replace(&mut *self.state(), TraceState::Stopping);
        match before {
            TraceState::Running(stopper) => stopper.stop(),
            TraceState::Idle | TraceState::Stopping => Ok(()),
        }
    }
}

/// Counts every frame presented, by process, until the session stops. Returns how many times the trace ran.
///
/// A trace that ends while the app is not stopping was stopped from outside; it is started again after a pause, which
/// is also what keeps a session that cannot start from being retried in a spin. The thread owns every session and
/// shares each one's stopper through `trace` while it runs, so the exit can end it.
fn count_frames_until_stopped(
    first: (TraceSession, TraceConsumer),
    session_name: &str,
    counter: &FrameCounter,
    trace: &FrameTrace<SessionStopper>,
    stop: &Receiver<()>,
) -> usize {
    let mut runs = 0;
    let mut current = first;
    loop {
        let (session, consumer) = current;
        if trace.begin(session.stopper()) {
            runs += 1;
            count_frames(consumer, counter);
            trace.end();
        }
        // The session may already be gone; dropping it makes sure no half-stopped one is left behind.
        drop(session);
        if trace.is_stopping() {
            return runs;
        }
        tracing::warn!("the frame trace ended, so it is started again");
        let Some(next) = start_again(session_name, trace, stop) else {
            return runs;
        };
        current = next;
    }
}

/// Waits and starts the session again until one starts; `None` once the app is stopping.
fn start_again(
    session_name: &str,
    trace: &FrameTrace<SessionStopper>,
    stop: &Receiver<()>,
) -> Option<(TraceSession, TraceConsumer)> {
    let mut last_failure = None;
    while wait_to_restart(stop, trace, RESTART_DELAY) {
        match start_session(session_name, PRESENT_PROVIDERS) {
            Ok(started) => return Some(started),
            Err(error) => {
                if is_new_failure(&mut last_failure, error.code().0) {
                    tracing::warn!(
                        ?error,
                        "the frame trace could not be started again; it is tried every 5 s"
                    );
                }
            }
        }
    }
    None
}

/// Whether a failed restart is worth a log line: the first is, then only one with another error code, so a trace that
/// cannot restart does not fill the log every 5 s.
fn is_new_failure(last: &mut Option<i32>, failure: i32) -> bool {
    last.replace(failure) != Some(failure)
}

/// Waits up to `delay` before a restart; `false` when the app is stopping, which a message or a gone sender says.
fn wait_to_restart<S: StopTrace>(stop: &Receiver<()>, trace: &FrameTrace<S>, delay: Duration) -> bool {
    match stop.recv_timeout(delay) {
        Err(RecvTimeoutError::Timeout) | Ok(()) => !trace.is_stopping(),
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

/// Counts the frames of one trace until its session stops.
fn count_frames(consumer: TraceConsumer, counter: &FrameCounter) {
    let outcome = consumer.run(&mut |event| {
        if is_present(event) {
            counter.record(event.process_id, event.timestamp_ns);
        }
    });
    if let Err(error) = outcome {
        tracing::warn!(?error, "frame counting stopped");
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../version_info.rs"]
mod version_info;
