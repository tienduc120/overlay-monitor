//! What the tray icon and the menus do to the overlay: a pure state machine, and the glue that applies its actions.

use std::sync::mpsc::{Receiver, Sender};

use monitor_application::{OverlayFrame, OverlayStyle, SamplerControl, Settings, SettingsStore, overlay_style};
use platform_win32::menu::{MenuItem, show_popup_menu};
use platform_win32::tray::TrayIcon;
use platform_win32::ui::{Flow, MessageWindow, UiEvent};

use crate::OverlayView;

const SHOW_OVERLAY: u16 = 1;
const EXIT: u16 = 2;

/// Whether the overlay is shown, the one fact the events and commands depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiState {
    pub visible: bool,
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
    /// Store whether the overlay is shown in the settings, and save them.
    SaveSettings,
    ReaddTrayIcon,
    Exit,
}

impl UiState {
    pub fn on_event(&mut self, event: UiEvent) -> Vec<Action> {
        match event {
            UiEvent::TrayClick => self.toggle(),
            UiEvent::TrayMenu { x, y } | UiEvent::OverlayMenu { x, y } => vec![Action::ShowMenu { x, y }],
            UiEvent::OverlayMoved => vec![Action::RememberPosition],
            UiEvent::Wake if self.visible => vec![Action::DrawLatest],
            UiEvent::Wake => Vec::new(),
            UiEvent::TaskbarRestarted => vec![Action::ReaddTrayIcon],
        }
    }

    pub fn on_command(&mut self, id: u16) -> Vec<Action> {
        match id {
            SHOW_OVERLAY => self.toggle(),
            EXIT => vec![Action::Exit],
            _ => Vec::new(),
        }
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
pub fn menu_items(visible: bool) -> Vec<MenuItem> {
    vec![
        MenuItem {
            id: SHOW_OVERLAY,
            text: String::from("Show overlay"),
            checked: visible,
        },
        MenuItem {
            id: EXIT,
            text: String::from("Exit"),
            checked: false,
        },
    ]
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
    settings: Settings,
    store: Box<dyn SettingsStore>,
    style: OverlayStyle,
    /// The newest frame received, drawn again when the overlay is shown or the style changes.
    latest: Option<OverlayFrame>,
    draw_errors: ErrorLog,
    tray_errors: ErrorLog,
}

impl Ui {
    /// Places the overlay where the settings say, styles it by them, and shows or hides it as they say.
    pub fn new(parts: UiParts) -> Self {
        let UiParts {
            view,
            tray,
            frames,
            sampler,
            settings,
            store,
        } = parts;
        view.place(settings.position.as_ref());
        if settings.visible {
            view.show();
        } else {
            view.hide();
        }
        Self {
            state: UiState {
                visible: settings.visible,
            },
            tray,
            view,
            frames,
            _sampler: sampler,
            style: overlay_style(&settings),
            settings,
            store,
            latest: None,
            draw_errors: ErrorLog::default(),
            tray_errors: ErrorLog::default(),
        }
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
            Action::RememberPosition => self.remember_position(),
            Action::SaveSettings => self.save_settings(),
            Action::ShowMenu { x, y } => return self.choose_from_menu(window, x, y),
            Action::Exit => return Flow::Exit,
        }
        Flow::Continue
    }

    /// Shows the menu and applies the command chosen from it, if any.
    fn choose_from_menu(&mut self, window: &MessageWindow, x: i32, y: i32) -> Flow {
        let items = menu_items(self.state.visible);
        match show_popup_menu(window, &items, x, y) {
            Some(id) => {
                let followed = self.state.on_command(id);
                self.apply(window, followed)
            }
            None => Flow::Continue,
        }
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
