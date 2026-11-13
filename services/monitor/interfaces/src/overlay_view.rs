use monitor_application::{OverlayFrame, OverlayStyle};
use platform_win32::Win32Error;
use platform_win32::overlay::OverlayWindow;
use platform_win32::text::TextStyle;

/// The overlay window, showing the sampler's frames.
pub struct OverlayView {
    window: OverlayWindow,
}

impl OverlayView {
    pub fn new(window: OverlayWindow) -> Self {
        Self { window }
    }

    /// Draws `frame` in `style`; the window keeps whatever visibility it had.
    ///
    /// # Errors
    /// The style's font is not installed, or drawing failed.
    pub fn show_frame(&mut self, frame: &OverlayFrame, style: &OverlayStyle) -> Result<(), Win32Error> {
        self.window.draw(&frame.lines, &text_style(style))
    }
}

/// The overlay's style as the window draws it.
pub fn text_style(style: &OverlayStyle) -> TextStyle {
    TextStyle {
        font_family: style.font_family.clone(),
        size_pt: style.size_pt,
        bold: style.bold,
        italic: style.italic,
        text_rgb: style.text_rgb,
        outline_rgb: style.outline_rgb,
        outline_px: style.outline_px,
        background_rgb: style.background_rgb,
        background_alpha: style.background_alpha,
    }
}

#[cfg(test)]
mod tests;
