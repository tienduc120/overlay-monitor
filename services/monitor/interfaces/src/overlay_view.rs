use monitor_application::{OverlayFrame, OverlayPosition, OverlayStyle};
use platform_win32::Win32Error;
use platform_win32::overlay::OverlayWindow;
use platform_win32::placement::MonitorPlacement;
use platform_win32::text::TextStyle;

/// The overlay window, showing the sampler's frames.
pub struct OverlayView {
    window: OverlayWindow,
    /// What the window shows now, so an unchanged frame is not drawn again.
    last: Option<(Vec<String>, OverlayStyle)>,
    /// The DPI of the monitor `last` was drawn on.
    drawn_dpi: u32,
}

impl OverlayView {
    pub fn new(window: OverlayWindow) -> Self {
        let drawn_dpi = window.dpi();
        Self {
            window,
            last: None,
            drawn_dpi,
        }
    }

    pub fn show(&self) {
        self.window.show();
    }

    pub fn hide(&self) {
        self.window.hide();
    }

    /// Lets clicks pass through the overlay (`on`) or catches them again.
    pub fn set_click_through(&self, on: bool) {
        self.window.set_click_through(on);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    /// Puts the overlay at `saved`, or at the main monitor's top-left when nothing is saved or its monitor is gone.
    pub fn place(&self, saved: Option<&OverlayPosition>) {
        let saved = saved.map(|position| MonitorPlacement {
            monitor: position.monitor.clone(),
            x: position.x,
            y: position.y,
        });
        self.window.move_to(saved.as_ref());
    }

    /// Where the overlay is now, or `None` when Windows cannot say.
    pub fn position(&self) -> Option<OverlayPosition> {
        self.window.placement().map(|placement| OverlayPosition {
            monitor: placement.monitor,
            x: placement.x,
            y: placement.y,
        })
    }

    /// Draws `frame` in `style` when it differs from what is shown, and otherwise only puts the window back on top
    /// of other topmost windows. The window keeps whatever visibility it had.
    ///
    /// # Errors
    /// The style's font is not installed, or drawing failed.
    pub fn show_frame(&mut self, frame: &OverlayFrame, style: &OverlayStyle) -> Result<(), Win32Error> {
        let dpi = self.window.dpi();
        if dpi_changed(self.drawn_dpi, dpi) {
            // The window's own redraw for the new scale may have failed, so draw whatever comes next in full.
            self.last = None;
        }
        let last = self.last.as_ref().map(|(lines, style)| (lines.as_slice(), style));
        if !needs_redraw(last, frame, style) {
            self.window.keep_on_top();
            return Ok(());
        }
        self.window.draw(&frame.lines, &text_style(style))?;
        // Only a drawn frame counts as shown, so a failed draw is tried again.
        self.last = Some((frame.lines.clone(), style.clone()));
        self.drawn_dpi = dpi;
        Ok(())
    }
}

/// Whether `frame` in `style` differs from `last`, what the window shows now.
pub fn needs_redraw(last: Option<(&[String], &OverlayStyle)>, frame: &OverlayFrame, style: &OverlayStyle) -> bool {
    last != Some((frame.lines.as_slice(), style))
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

/// Whether the window moved to a monitor with another scale since `drawn`, which its own redraw may have failed.
fn dpi_changed(drawn: u32, now: u32) -> bool {
    drawn != now
}

#[cfg(test)]
mod tests;
