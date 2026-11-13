use monitor_domain::Style;

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
    let style = Style::default();
    OverlayStyle {
        background_alpha: style.background_alpha(false),
        font_family: style.font_family,
        size_pt: style.size_pt,
        bold: style.bold,
        italic: style.italic,
        text_rgb: style.text.0,
        outline_rgb: style.outline.0,
        outline_px: style.outline_px,
        background_rgb: style.background.0,
    }
}
