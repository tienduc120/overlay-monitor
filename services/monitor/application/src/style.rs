use crate::{Settings, overlay_style};

/// The overlay's look as plain values, for delivery to draw with.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayStyle {
    pub font_family: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub text_rgb: u32,
    pub outline_rgb: u32,
    pub outline_px: f32,
    pub background_rgb: u32,
    pub background_alpha: u8,
}

/// The default look, with the alpha of an unlocked overlay.
pub fn default_style() -> OverlayStyle {
    overlay_style(&Settings::default())
}
