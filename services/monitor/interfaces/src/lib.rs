//! Delivery for the monitor: the overlay window drawing the [`monitor_application::Sampler`]'s frames in the chosen
//! style.

mod overlay_view;
mod settings_editor;
mod settings_window;
mod shortcuts;
mod ui;

pub use overlay_view::{OverlayView, needs_redraw, text_style};
pub use settings_editor::{Change, Colour, Problem, SettingsEditor, ShortcutSlot};
pub use shortcuts::{Shortcuts, hotkey_keys, shortcut_keys};
pub use ui::{Action, Ui, UiParts, UiState, menu_items, restore_focus_after};
