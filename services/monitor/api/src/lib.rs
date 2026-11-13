//! The composition root: the one place that knows which adapter backs each sensor. It installs the subscriber,
//! opens the windows, and runs the sampler and the frame counter beside the UI loop.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};

use monitor_application::{
    DriverStatus, GpuPair, GpuSensors, OverlayFrame, Sampler, SamplerControl, Sensor, Sensors, Settings, SettingsStore,
    StartupTask, StartupTaskError, load_settings,
};
use monitor_infrastructure::{
    AdapterSensors, CpuLoad, Ddr5Temperature, FrameCounter, FrameRate, MemoryLoad, PRESENT_PROVIDERS, PawnIoProbe,
    SchtasksStartup, TASK_NAME, TomlSettingsStore, UnavailableSensor, cpu_temperature, is_present, memory_kind,
    memory_text, memory_types, reopening, settings_path,
};
use monitor_interfaces::{OverlayView, Ui, UiParts};
use platform_clock::{Clock, SystemClock};
use platform_win32::Win32Error;
use platform_win32::account::current_user_sid;
use platform_win32::cpu::cpu_identity;
use platform_win32::dpi::{enable_per_monitor_awareness, small_icon_size};
use platform_win32::etw::{TraceConsumer, TraceSession, start_session, stop_session};
use platform_win32::firmware::smbios_table;
use platform_win32::gpu::{GpuAdapter, gpu_adapters};
use platform_win32::icon::Icon;
use platform_win32::instance::SingleInstance;
use platform_win32::known_folder::roaming_app_data;
use platform_win32::overlay::OverlayWindow;
use platform_win32::tray::TrayIcon;
use platform_win32::ui::{MessageWindow, run_message_loop};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

/// Held while a copy runs, so a second copy in this session exits at once.
const INSTANCE_NAME: &str = r"Local\OverlayMonitor";
const TRACE_SESSION: &str = "OverlayMonitor";
const TOOLTIP: &str = "Overlay Monitor";
/// The owner's icon, in the file's several sizes.
const OWNER_ICON: &[u8] = include_bytes!("../assets/icon.ico");
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
    install_subscriber();
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
            tracing::error!(?error, "claim the single-instance name");
            return ExitCode::FAILURE;
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
    let gpu_names = adapters.iter().map(|adapter| adapter.name.clone()).collect();
    let mut gpus = AdapterSensors::new(adapters);
    let gpu = gpus.open(settings.gpu.as_deref());
    let trace = start_frame_trace();
    let counter = FrameCounter::new();
    let mut sensors = build_sensors(&own_modules_dir(), gpu, Arc::clone(&counter));
    if trace.is_none() {
        sensors.fps = unavailable(FPS_NEEDS_ADMIN);
    }
    let sampler = Sampler::new(sensors, Box::new(gpus), SystemClock, &settings);
    match show_overlay(sampler, settings, store, gpu_names, trace, &counter) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(?error, "open the overlay");
            ExitCode::FAILURE
        }
    }
}

fn install_subscriber() {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    #[expect(clippy::disallowed_methods, reason = "the composition root installs the subscriber")]
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
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

/// Opens the overlay and the tray icon, runs the sampler and the frame counter beside the UI loop until it ends, then
/// stops them in order.
fn show_overlay(
    mut sampler: Sampler<SystemClock>,
    settings: Settings,
    store: Box<dyn SettingsStore>,
    gpu_names: Vec<String>,
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
            gpu_names,
            startup: startup_task(),
            driver: Box::new(PawnIoProbe),
        },
    );
    let waker = window.waker();
    let (session, consumer) = trace.unzip();
    std::thread::scope(|scope| {
        scope.spawn(move || sample_until_stopped(&mut sampler, &frames, || waker.wake(), &controls));
        if let Some(consumer) = consumer {
            scope.spawn(move || count_frames(consumer, counter));
        }
        run_message_loop(&window, &mut |event| ui.handle(&window, event));
        // The scope joins both threads next: the UI holds the control sender, so dropping it lets the sampler end, and
        // the consumer's run ends with the session, even when stopping it fails.
        drop(ui);
        if let Some(session) = &session
            && let Err(error) = session.stop()
        {
            tracing::warn!(?error, "stop the frame trace");
        }
    });
    // The tray icon, which `ui` held, went before the window the shell knows it by; the image it shows drops last.
    drop(window);
    Ok(())
}

/// The owner's icon at the tray's size, or the stock one when it cannot be made.
fn tray_icon_image() -> Result<Icon, Win32Error> {
    Icon::from_ico(OWNER_ICON, small_icon_size())
        .inspect_err(|error| tracing::warn!(?error, "use the stock tray icon, as the owner's could not be made"))
        .or_else(|_| Icon::stock())
}

/// Sends a frame and wakes the UI to draw it once an interval, and again at once after each `Apply`, until the control
/// sender is gone.
fn sample_until_stopped<C: Clock>(
    sampler: &mut Sampler<C>,
    frames: &Sender<OverlayFrame>,
    wake: impl Fn(),
    control: &Receiver<SamplerControl>,
) {
    loop {
        if frames.send(sampler.sample()).is_err() {
            // Nobody is left to draw a frame.
            return;
        }
        wake();
        match control.recv_timeout(sampler.interval()) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(SamplerControl::Apply(settings)) => sampler.apply(&settings),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Counts every frame presented, by process, until the session stops.
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
