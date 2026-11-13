//! What the tray icon and the menus do to the overlay: a pure state machine, and the glue that applies its actions.

use std::sync::mpsc::Receiver;

use monitor_application::{OverlayFrame, OverlayStyle};
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
    ShowMenu { x: i32, y: i32 },
    DrawLatest,
    ReaddTrayIcon,
    Exit,
}

impl UiState {
    pub fn on_event(&mut self, event: UiEvent) -> Vec<Action> {
        match event {
            UiEvent::TrayClick => self.toggle(),
            UiEvent::TrayMenu { x, y } | UiEvent::OverlayMenu { x, y } => vec![Action::ShowMenu { x, y }],
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
            vec![Action::ShowOverlay, Action::DrawLatest]
        } else {
            vec![Action::HideOverlay]
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

/// The UI thread's state: the overlay, the tray icon, and the sampler's frames.
pub struct Ui {
    state: UiState,
    // The tray icon goes before the window that owns it, and the caller drops the window after the `Ui`.
    tray: TrayIcon,
    view: OverlayView,
    frames: Receiver<OverlayFrame>,
    style: OverlayStyle,
    /// The newest frame received, drawn again when the overlay is shown or the style changes.
    latest: Option<OverlayFrame>,
}

impl Ui {
    pub fn new(view: OverlayView, tray: TrayIcon, frames: Receiver<OverlayFrame>, style: OverlayStyle) -> Self {
        Self {
            state: UiState {
                visible: view.is_visible(),
            },
            tray,
            view,
            frames,
            style,
            latest: None,
        }
    }

    pub fn handle(&mut self, window: &MessageWindow, event: UiEvent) -> Flow {
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
            // The icon may still be there, or the shell may not be ready; the next restart notice tries again.
            Action::ReaddTrayIcon => drop(self.tray.readd()),
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

    fn draw_latest(&mut self) {
        if let Some(newest) = self.frames.try_iter().last() {
            self.latest = Some(newest);
        }
        if let Some(frame) = &self.latest {
            // A missing font or a failed present keeps the old picture; the next frame tries again.
            drop(self.view.show_frame(frame, &self.style));
        }
    }
}

#[cfg(test)]
mod tests;
