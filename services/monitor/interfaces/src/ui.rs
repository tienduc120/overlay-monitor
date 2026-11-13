//! What the tray icon and the menus do to the overlay: a pure state machine, and the glue that applies its actions.

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use monitor_application::{
    OverlayFrame, OverlayStyle, SamplerControl, Settings, SettingsStore, blank_frame, overlay_style,
};
use platform_win32::foreground::ForegroundWindow;
use platform_win32::menu::{MenuItem, show_popup_menu};
use platform_win32::tray::TrayIcon;
use platform_win32::ui::{Flow, MessageWindow, UiEvent};

use crate::OverlayView;
use crate::shortcuts::{LOCK_HOTKEY, SHOW_HIDE_HOTKEY, Shortcuts};

const SHOW_OVERLAY: u16 = 1;
const EXIT: u16 = 2;
const LOCK: u16 = 3;

/// Whether the overlay is shown and whether it is locked in place, the facts the events and commands depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiState {
    pub visible: bool,
    /// A locked overlay lets clicks through, so it cannot be dragged.
    pub locked: bool,
}

/// What the UI thread must do next, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ShowOverlay,
    HideOverlay,
    ShowMenu {
        x: i32,
        y: i32,
    },
    DrawLatest,
    /// Store where the overlay is now in the settings, and save them.
    RememberPosition,
    /// Lock or unlock the overlay: clicks pass through while it is locked, and its style follows.
    SetLocked(bool),
    /// Store whether the overlay is shown and locked in the settings, and save them.
    SaveSettings,
    ReaddTrayIcon,
    /// Blank the overlay when the sampler has gone quiet.
    CheckStalled,
    Exit,
}

impl UiState {
    pub fn on_event(&mut self, event: UiEvent) -> Vec<Action> {
        match event {
            UiEvent::TrayClick | UiEvent::Hotkey { id: SHOW_HIDE_HOTKEY } => self.toggle(),
            UiEvent::TrayMenu { x, y } | UiEvent::OverlayMenu { x, y } => vec![Action::ShowMenu { x, y }],
            UiEvent::OverlayMoved => vec![Action::RememberPosition],
            UiEvent::Hotkey { id: LOCK_HOTKEY } => self.toggle_lock(),
            UiEvent::Wake if self.visible => vec![Action::DrawLatest],
            UiEvent::Tick if self.visible => vec![Action::CheckStalled],
            UiEvent::Hotkey { .. } | UiEvent::Wake | UiEvent::Tick => Vec::new(),
            UiEvent::TaskbarRestarted => vec![Action::ReaddTrayIcon],
            UiEvent::SessionEnding => vec![Action::Exit],
        }
    }

    pub fn on_command(&mut self, id: u16) -> Vec<Action> {
        match id {
            SHOW_OVERLAY => self.toggle(),
            LOCK => self.toggle_lock(),
            EXIT => vec![Action::Exit],
            _ => Vec::new(),
        }
    }

    /// The style changes with the lock, so the newest frame is drawn again in it.
    fn toggle_lock(&mut self) -> Vec<Action> {
        self.locked = !self.locked;
        vec![Action::SetLocked(self.locked), Action::DrawLatest, Action::SaveSettings]
    }

    /// A shown overlay may be showing a stale frame, so showing it draws the newest one at once.
    fn toggle(&mut self) -> Vec<Action> {
        self.visible = !self.visible;
        if self.visible {
            vec![Action::ShowOverlay, Action::DrawLatest, Action::SaveSettings]
        } else {
            vec![Action::HideOverlay, Action::SaveSettings]
        }
    }
}

/// The menu both the tray icon and the overlay open.
pub fn menu_items(state: &UiState) -> Vec<MenuItem> {
    vec![
        MenuItem {
            id: SHOW_OVERLAY,
            text: String::from("Show overlay"),
            checked: state.visible,
        },
        MenuItem {
            id: LOCK,
            text: String::from("Lock position"),
            checked: state.locked,
        },
        MenuItem {
            id: EXIT,
            text: String::from("Exit"),
            checked: false,
        },
    ]
}

/// Whether the window that was in front before the menu opened gets the focus back, given what was chosen (`None`
/// when the menu was dismissed). Every choice does for now.
pub fn restore_focus_after(_command: Option<u16>) -> bool {
    true
}

/// Whether more than three update intervals passed since the last frame, so the numbers on screen are old.
pub(crate) fn frames_stalled(last_frame: Instant, now: Instant, interval: Duration) -> bool {
    now.saturating_duration_since(last_frame) > interval.saturating_mul(3)
}

/// Remembers the last failure of one kind of operation, so a failure that repeats every frame is logged once.
#[derive(Debug, Default)]
struct ErrorLog {
    last: Option<i32>,
}

impl ErrorLog {
    /// Whether this outcome is worth a log line: `failure` is the error code, or `None` for success. A different
    /// code is news, the same one again is not, and a success makes the next failure news again.
    fn is_new(&mut self, failure: Option<i32>) -> bool {
        let news = failure.is_some() && failure != self.last;
        self.last = failure;
        news
    }
}

/// What the UI thread is built from: the windows, the sampler's two channels, and the settings it starts from and
/// saves into.
pub struct UiParts {
    pub view: OverlayView,
    pub tray: TrayIcon,
    pub frames: Receiver<OverlayFrame>,
    pub sampler: Sender<SamplerControl>,
    pub settings: Settings,
    pub store: Box<dyn SettingsStore>,
}

/// The UI thread's state: the overlay, the tray icon, the sampler's frames, and the settings.
pub struct Ui {
    state: UiState,
    // The tray icon goes before the window that owns it, and the caller drops the window after the `Ui`.
    tray: TrayIcon,
    view: OverlayView,
    frames: Receiver<OverlayFrame>,
    /// Held so that dropping the `Ui` is what tells the sampler to stop; settings changes will be sent through it.
    _sampler: Sender<SamplerControl>,
    /// Held so the shortcuts stay registered until the `Ui` is dropped.
    _shortcuts: Shortcuts,
    settings: Settings,
    store: Box<dyn SettingsStore>,
    style: OverlayStyle,
    /// The newest frame received, drawn again when the overlay is shown or the style changes.
    latest: Option<OverlayFrame>,
    /// When a frame last arrived, for the stall check.
    last_frame: Instant,
    draw_errors: ErrorLog,
    tray_errors: ErrorLog,
}

impl Ui {
    /// Registers the shortcuts, places the overlay where the settings say, styles it by them, locks it and shows or
    /// hides it as they say.
    pub fn new(window: &MessageWindow, parts: UiParts) -> Self {
        let UiParts {
            view,
            tray,
            frames,
            sampler,
            mut settings,
            store,
        } = parts;
        let shortcuts = Shortcuts::register(window, &settings);
        // A locked overlay can only be unlocked from the tray menu or by its shortcut; without the shortcut it starts
        // unlocked, so it can always be grabbed.
        let lock_dropped = settings.locked && !shortcuts.lock_unlock_registered();
        if lock_dropped {
            tracing::warn!("the lock shortcut is not available, so the overlay starts unlocked");
            settings.locked = false;
        }
        view.place(settings.position.as_ref());
        view.set_click_through(settings.locked);
        if settings.visible {
            view.show();
        } else {
            view.hide();
        }
        let mut ui = Self {
            state: UiState {
                visible: settings.visible,
                locked: settings.locked,
            },
            tray,
            view,
            frames,
            _sampler: sampler,
            _shortcuts: shortcuts,
            style: overlay_style(&settings),
            settings,
            store,
            latest: None,
            last_frame: Instant::now(),
            draw_errors: ErrorLog::default(),
            tray_errors: ErrorLog::default(),
        };
        if lock_dropped {
            ui.save_settings();
        }
        // The shell may not be ready at startup; the icon then comes with the taskbar.
        if let Err(error) = ui.tray.readd() {
            tracing::info!(?error, "the tray icon is not added yet; it comes with the taskbar");
        }
        if let Err(error) = window.set_tick(ui.settings.update_interval) {
            tracing::warn!(
                ?error,
                "a stalled sampler will not blank the overlay, as the timer could not be set"
            );
        }
        ui
    }

    pub fn handle(&mut self, window: &MessageWindow, event: UiEvent) -> Flow {
        // Whether or not the overlay is shown, so frames sent while it is hidden do not pile up.
        if event == UiEvent::Wake {
            self.take_frames();
        }
        let actions = self.state.on_event(event);
        self.apply(window, actions)
    }

    fn apply(&mut self, window: &MessageWindow, actions: Vec<Action>) -> Flow {
        for action in actions {
            if self.apply_one(window, action) == Flow::Exit {
                return Flow::Exit;
            }
        }
        Flow::Continue
    }

    fn apply_one(&mut self, window: &MessageWindow, action: Action) -> Flow {
        match action {
            Action::ShowOverlay => self.view.show(),
            Action::HideOverlay => self.view.hide(),
            Action::DrawLatest => self.draw_latest(),
            Action::ReaddTrayIcon => self.readd_tray_icon(),
            Action::CheckStalled => self.check_stalled(),
            Action::RememberPosition => self.remember_position(),
            Action::SaveSettings => self.save_settings(),
            Action::SetLocked(locked) => self.set_locked(locked),
            Action::ShowMenu { x, y } => return self.choose_from_menu(window, x, y),
            Action::Exit => return Flow::Exit,
        }
        Flow::Continue
    }

    /// Shows the menu and applies the command chosen from it, if any. Whatever window the user was in goes back in
    /// front once it closes, because showing the menu made this app's hidden window the foreground.
    fn choose_from_menu(&mut self, window: &MessageWindow, x: i32, y: i32) -> Flow {
        let items = menu_items(&self.state);
        let before = ForegroundWindow::current();
        let choice = show_popup_menu(window, &items, x, y);
        if restore_focus_after(choice)
            && let Some(before) = before
        {
            // A window that closed meanwhile, or a refusal, leaves the focus where Windows put it.
            before.restore();
        }
        match choice {
            Some(id) => {
                let followed = self.state.on_command(id);
                self.apply(window, followed)
            }
            None => Flow::Continue,
        }
    }

    fn set_locked(&mut self, locked: bool) {
        self.settings.locked = locked;
        self.view.set_click_through(locked);
        self.style = overlay_style(&self.settings);
    }

    /// Keeps the place the overlay was dragged to. A position Windows cannot report leaves the saved one as it was.
    fn remember_position(&mut self) {
        if let Some(position) = self.view.position() {
            self.settings.position = Some(position);
        }
        self.save_settings();
    }

    /// A failed save is logged and the app carries on with what it has in memory.
    fn save_settings(&mut self) {
        self.settings.visible = self.state.visible;
        self.settings.locked = self.state.locked;
        if let Err(error) = self.store.save(&self.settings) {
            tracing::warn!(?error, "the settings could not be saved");
        }
    }

    fn readd_tray_icon(&mut self) {
        // The icon may still be there, or the shell not ready yet; the next restart notice tries again.
        let failure = self.tray.readd().err();
        if self.tray_errors.is_new(failure.as_ref().map(|e| e.code().0)) {
            tracing::warn!(error = ?failure, "the tray icon could not be added again");
        }
    }

    /// Moves what the sampler sent into `latest`, keeping only the newest so the channel never grows.
    fn take_frames(&mut self) {
        if let Some(newest) = self.frames.try_iter().last() {
            self.latest = Some(newest);
            self.last_frame = Instant::now();
        }
    }

    /// A sampler that stopped delivering must not leave old numbers looking current. The dashes stay as the latest
    /// frame until a real one arrives, so a redraw (lock, show) keeps them.
    fn check_stalled(&mut self) {
        if !frames_stalled(self.last_frame, Instant::now(), self.settings.update_interval) {
            return;
        }
        let blank = blank_frame(&self.settings);
        if self.latest.as_ref() != Some(&blank) {
            self.latest = Some(blank);
            self.draw_latest();
        }
    }

    fn draw_latest(&mut self) {
        self.take_frames();
        let Some(frame) = &self.latest else { return };
        // A missing font or a failed present keeps the old picture; the next frame tries again.
        let failure = self.view.show_frame(frame, &self.style).err();
        if self.draw_errors.is_new(failure.as_ref().map(|e| e.code().0)) {
            tracing::warn!(error = ?failure, "the overlay could not draw its frame");
        }
    }
}

#[cfg(test)]
mod tests;
