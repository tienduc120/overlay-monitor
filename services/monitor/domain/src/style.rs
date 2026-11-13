use alloc::string::String;

/// A colour as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub font_family: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub text: Rgb,
    pub outline: Rgb,
    pub outline_px: f32,
    pub background: Rgb,
    pub background_opacity_percent: u8,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font_family: String::from("Consolas"),
            size_pt: 14.0,
            bold: true,
            italic: false,
            text: Rgb(0x0000_FF00),
            outline: Rgb(0x0000_0000),
            outline_px: 2.0,
            background: Rgb(0x0000_0000),
            background_opacity_percent: 0,
        }
    }
}

impl Style {
    /// An unlocked overlay keeps at least alpha 1: fully transparent pixels let clicks through, so it could not be
    /// dragged.
    pub fn background_alpha(&self, locked: bool) -> u8 {
        let percent = u16::from(self.background_opacity_percent.min(100));
        let alpha = u8::try_from((percent * 255 + 50) / 100).unwrap_or(u8::MAX);
        if locked { alpha } else { alpha.max(1) }
    }
}

#[cfg(test)]
mod tests;
