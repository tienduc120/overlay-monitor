//! Delivery for the monitor: the overlay window drawing the [`monitor_application::Sampler`]'s frames in the chosen
//! style.

mod overlay_view;
mod ui;

pub use overlay_view::{OverlayView, needs_redraw, text_style};
pub use ui::{Action, Ui, UiParts, UiState, menu_items};
