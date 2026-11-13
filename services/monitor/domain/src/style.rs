use alloc::format;
use alloc::string::String;

/// A colour as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u32);

impl Rgb {
    /// Reads `#RRGGBB` or `RRGGBB`; anything else is `None`.
    #[must_use]
    pub fn parse_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        // from_str_radix would also take a leading sign.
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(digits, 16).ok().map(Self)
    }

    #[must_use]
    pub fn hex(self) -> String {
        format!("#{:06X}", self.0 & 0x00FF_FFFF)
    }
}

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
    /// The style with every value pulled into a range the overlay can draw, for a settings file edited by hand.
    #[must_use]
    pub fn clamped(self) -> Self {
        let defaults = Self::default();
        let family = self.font_family.trim();
        Self {
            font_family: String::from(if family.is_empty() { "Consolas" } else { family }),
            size_pt: if self.size_pt.is_nan() {
                defaults.size_pt
            } else {
                self.size_pt.clamp(6.0, 72.0)
            },
            outline_px: if self.outline_px.is_nan() {
                defaults.outline_px
            } else {
                self.outline_px.clamp(0.0, 8.0)
            },
            background_opacity_percent: self.background_opacity_percent.min(100),
            ..self
        }
    }
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
