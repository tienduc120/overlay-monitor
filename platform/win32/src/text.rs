//! Outlined text: DirectWrite rasterizes each line's glyphs into a coverage map, which grows into the outline's; the
//! outline is painted in its colour and the glyphs over it in the text colour, so the text reads on any background.
//! Nothing but DirectWrite is loaded to draw it.

use std::mem::ManuallyDrop;
use std::ptr;

use windows::Win32::Foundation::{DWRITE_E_NOFONT, E_INVALIDARG, E_POINTER};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_GLYPH_METRICS,
    DWRITE_GLYPH_RUN, DWRITE_GRID_FIT_MODE_DISABLED, DWRITE_MATRIX, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE, DWRITE_TEXTURE_ALIASED_1x1,
    DWriteCreateFactory, IDWriteFactory, IDWriteFactory2, IDWriteFontFace, IDWriteLocalizedStrings,
};
use windows::core::{BOOL, HSTRING, PCWSTR};

use crate::Win32Error;
use crate::coverage::{Coverage, blend};

/// How the overlay's text looks. Colours are `0xRRGGBB`; sizes are in points (`size_pt`) and pixels (`outline_px`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
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

/// How many times finer than the canvas glyphs are rasterized at most. DirectWrite's grayscale texture has 16 levels
/// and puts glyphs on a quarter-pixel grid; four times finer and averaged back, the edges are as smooth and the glyphs
/// as exactly placed as their outlines.
const MAX_OVERSAMPLE: u8 = 4;
/// The finest grid worth rasterizing on, in fine pixels per em. Past it a large glyph's edges are already smooth, and
/// a finer grid only costs time: four times finer is 16 times the pixels.
const FINE_PX_PER_EM: f32 = 256.0;

/// Places lines of text as glyphs and works out how much of each pixel they cover.
pub(crate) struct TextPainter {
    /// Version 2 rasterizes in grayscale without hinting: the glyphs' own outlines, as they were drawn before.
    dwrite: IDWriteFactory2,
}

/// Lines of text as glyphs, already in place on the canvas.
pub(crate) struct PlacedText {
    face: IDWriteFontFace,
    em_px: f32,
    lines: Vec<PlacedLine>,
    /// The text's size in pixels, without the outline.
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// One line's glyphs, their advances in pixels, and where they start.
struct PlacedLine {
    glyphs: Vec<u16>,
    advances: Vec<f32>,
    place: Placement,
}

/// One line's place on the canvas: where its glyphs start, and its baseline.
struct Placement {
    left: f32,
    baseline: f32,
}

impl TextPainter {
    pub(crate) fn new() -> Result<Self, Win32Error> {
        // SAFETY: the call takes only the factory type.
        let dwrite: IDWriteFactory2 = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        Ok(Self { dwrite })
    }

    /// Places `lines` in `style`'s font at `em_px` pixels per em, with the text's top-left corner at (`left`, `top`).
    ///
    /// # Errors
    /// `DWRITE_E_NOFONT` when the font family is not installed; otherwise a DirectWrite failure.
    pub(crate) fn lay_out(
        &self,
        lines: &[String],
        style: &TextStyle,
        em_px: f32,
        (left, top): (f32, f32),
    ) -> Result<PlacedText, Win32Error> {
        let face = self.font_face(style)?;
        let mut metrics = DWRITE_FONT_METRICS::default();
        // SAFETY: `metrics` is a live local the call fills.
        unsafe { face.GetMetrics(&raw mut metrics) };
        let px_per_unit = em_px / f32::from(metrics.designUnitsPerEm);
        let ascent = f32::from(metrics.ascent) * px_per_unit;
        let descent = f32::from(metrics.descent) * px_per_unit;
        let line_advance = ascent + descent + f32::from(metrics.lineGap) * px_per_unit;

        let mut placed = Vec::with_capacity(lines.len());
        let (mut width, mut bottom) = (0.0_f32, top);
        let mut baseline = top + ascent;
        for line in lines {
            // A whole-pixel baseline keeps every line's horizontal strokes equally sharp.
            let place = Placement {
                left,
                baseline: baseline.round(),
            };
            let line = place_line(&face, line, px_per_unit, place)?;
            width = width.max(line.advances.iter().sum());
            placed.push(line);
            bottom = baseline + descent;
            baseline += line_advance;
        }
        Ok(PlacedText {
            face,
            em_px,
            lines: placed,
            width,
            height: bottom - top,
        })
    }

    /// How much of each pixel of a `width` by `height` canvas the glyphs of `text` cover, anti-aliased in grayscale.
    ///
    /// # Errors
    /// DirectWrite could not rasterize a line.
    pub(crate) fn coverage(&self, text: &PlacedText, width: usize, height: usize) -> Result<Coverage, Win32Error> {
        let mut coverage = Coverage::new(width, height);
        for line in &text.lines {
            self.rasterize(text, line, &mut coverage)?;
        }
        Ok(coverage)
    }

    /// Adds one line's glyphs to `coverage`: grayscale, without hinting, at the line's place to a fraction of a pixel.
    fn rasterize(&self, text: &PlacedText, line: &PlacedLine, coverage: &mut Coverage) -> Result<(), Win32Error> {
        let count = u32::try_from(line.glyphs.len())
            .ok()
            .ok_or_else(|| Win32Error::from(E_INVALIDARG))?;
        if count == 0 {
            return Ok(());
        }
        let run = DWRITE_GLYPH_RUN {
            fontFace: ManuallyDrop::new(Some(text.face.clone())),
            fontEmSize: text.em_px,
            glyphCount: count,
            glyphIndices: line.glyphs.as_ptr(),
            glyphAdvances: line.advances.as_ptr(),
            glyphOffsets: ptr::null(),
            isSideways: false.into(),
            bidiLevel: 0,
        };
        let oversample = oversample(text.em_px);
        let finer = f32::from(oversample);
        let scale = DWRITE_MATRIX {
            m11: finer,
            m22: finer,
            ..DWRITE_MATRIX::default()
        };
        // SAFETY: the run's face is alive, its glyph and advance buffers hold `count` elements each, and they and the
        // transform outlive the call; the run has no offsets.
        let analysis = unsafe {
            self.dwrite.CreateGlyphRunAnalysis(
                &raw const run,
                Some(&raw const scale),
                DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
                DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_GRID_FIT_MODE_DISABLED,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                line.place.left,
                line.place.baseline,
            )
        };
        // The run holds a reference of its own to the face, which `ManuallyDrop` would otherwise leak.
        drop(ManuallyDrop::into_inner(run.fontFace));
        let analysis = analysis?;
        // In grayscale a 1x1 texture holds one coverage byte per pixel.
        // SAFETY: the analysis is alive.
        let bounds = unsafe { analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1) }?;
        let (Ok(width), Ok(height)) = (
            usize::try_from(bounds.right - bounds.left),
            usize::try_from(bounds.bottom - bounds.top),
        ) else {
            return Ok(());
        };
        let size = width
            .checked_mul(height)
            .ok_or_else(|| Win32Error::from(E_INVALIDARG))?;
        if size == 0 {
            // Only blanks: nothing to cover.
            return Ok(());
        }
        let mut texture = vec![0_u8; size];
        // SAFETY: `bounds` is a live local, and the buffer holds the one byte per pixel of it that a 1x1 texture takes.
        unsafe { analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &raw const bounds, &mut texture) }?;
        coverage.add_scaled(&texture, width, (bounds.left, bounds.top), oversample);
        Ok(())
    }

    /// The installed face of `style`'s family nearest its weight and slant. A family without a bold or italic face
    /// gets one simulated.
    fn font_face(&self, style: &TextStyle) -> Result<IDWriteFontFace, Win32Error> {
        let mut fonts = None;
        // SAFETY: `fonts` is a live local the call fills.
        unsafe { self.dwrite.GetSystemFontCollection(&raw mut fonts, false) }?;
        let fonts = fonts.ok_or_else(|| Win32Error::from(E_POINTER))?;
        let family = HSTRING::from(style.font_family.as_str());
        let (mut index, mut exists) = (0, BOOL::default());
        // SAFETY: `family` is NUL-terminated and outlives the call; `index` and `exists` are live locals.
        unsafe { fonts.FindFamilyName(&family, &raw mut index, &raw mut exists) }?;
        if !exists.as_bool() {
            return Err(DWRITE_E_NOFONT.into());
        }
        // SAFETY: `index` names a family of this collection.
        let family = unsafe { fonts.GetFontFamily(index) }?;
        let weight = if style.bold {
            DWRITE_FONT_WEIGHT_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        let slant = if style.italic {
            DWRITE_FONT_STYLE_ITALIC
        } else {
            DWRITE_FONT_STYLE_NORMAL
        };
        // SAFETY: the arguments are plain values.
        let font = unsafe { family.GetFirstMatchingFont(weight, DWRITE_FONT_STRETCH_NORMAL, slant) }?;
        // SAFETY: the font is alive; the face carries the font's simulations.
        unsafe { font.CreateFontFace() }
    }
}

/// Paints outlined text whose glyphs cover `fill` over premultiplied BGRA `pixels`, which are the coverage's: the
/// outline, `outline_px` past every edge of the glyphs with round corners, then the glyphs in the text colour. Every
/// line's outline goes first, so it never covers the letters of the line above it.
pub(crate) fn paint(pixels: &mut [[u8; 4]], fill: &Coverage, style: &TextStyle) {
    if style.outline_px > 0.0 {
        blend(pixels, &fill.grown(style.outline_px), style.outline_rgb);
    }
    blend(pixels, fill, style.text_rgb);
}

/// How many times finer than the canvas to rasterize glyphs of `em_px` pixels per em: as fine as
/// [`MAX_OVERSAMPLE`] allows without passing [`FINE_PX_PER_EM`], and never coarser than the canvas.
fn oversample(em_px: f32) -> u8 {
    (1..=MAX_OVERSAMPLE)
        .rev()
        .find(|&times| em_px * f32::from(times) <= FINE_PX_PER_EM)
        .unwrap_or(1)
}

/// One line's glyphs in `face`, with their advances in pixels, at `place`.
fn place_line(
    face: &IDWriteFontFace,
    line: &str,
    px_per_unit: f32,
    place: Placement,
) -> Result<PlacedLine, Win32Error> {
    let code_points: Vec<u32> = line.chars().map(u32::from).collect();
    let count = u32::try_from(code_points.len())
        .ok()
        .ok_or_else(|| Win32Error::from(E_INVALIDARG))?;
    let mut glyphs = vec![0_u16; code_points.len()];
    // SAFETY: both buffers hold `count` elements.
    unsafe { face.GetGlyphIndices(code_points.as_ptr(), count, glyphs.as_mut_ptr()) }?;
    let mut metrics = vec![DWRITE_GLYPH_METRICS::default(); glyphs.len()];
    // SAFETY: both buffers hold `count` elements.
    unsafe { face.GetDesignGlyphMetrics(glyphs.as_ptr(), count, metrics.as_mut_ptr(), false) }?;
    let advances = metrics
        .iter()
        .map(|glyph| design_units(glyph.advanceWidth) * px_per_unit)
        .collect();
    Ok(PlacedLine {
        glyphs,
        advances,
        place,
    })
}

/// A glyph measure in font design units. OpenType stores advances as 16-bit values, which convert to f32 exactly.
fn design_units(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

/// What the user's locale is called when Windows cannot say.
const FALLBACK_LOCALE: &str = "en-us";
/// `LOCALE_NAME_MAX_LENGTH`, with its terminator.
const LOCALE_NAME_CAPACITY: usize = 85;

/// The names of the font families the overlay can draw, one per family in the system collection, each in the user's
/// locale (else en-us, else its first name). They are the same names the overlay's text painter finds a family by.
///
/// # Errors
/// DirectWrite could not be created or would not give the system font collection.
pub fn installed_font_families() -> Result<Vec<String>, Win32Error> {
    // SAFETY: the call takes only the factory type.
    let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
    let mut fonts = None;
    // SAFETY: `fonts` is a live local the call fills.
    unsafe { dwrite.GetSystemFontCollection(&raw mut fonts, false) }?;
    let fonts = fonts.ok_or_else(|| Win32Error::from(E_POINTER))?;
    let locale = user_locale();
    // SAFETY: the collection is alive.
    let count = unsafe { fonts.GetFontFamilyCount() };
    // A family that will not give its names is left out rather than failing the whole list.
    Ok((0..count)
        .filter_map(|index| {
            // SAFETY: `index` is below the collection's count.
            let family = unsafe { fonts.GetFontFamily(index) }.ok()?;
            // SAFETY: the family is alive.
            let names = unsafe { family.GetFamilyNames() }.ok()?;
            localized_name(&names, &locale)
        })
        .collect())
}

/// The user's locale name as a NUL-terminated string, or [`FALLBACK_LOCALE`].
fn user_locale() -> Vec<u16> {
    let mut name = [0_u16; LOCALE_NAME_CAPACITY];
    // SAFETY: the buffer is a live local of the length Windows documents for a locale name.
    let written = unsafe { GetUserDefaultLocaleName(&mut name) };
    match usize::try_from(written) {
        Ok(length) if length > 1 => name.get(..length).map(<[u16]>::to_vec).unwrap_or_default(),
        _ => FALLBACK_LOCALE.encode_utf16().chain(Some(0)).collect(),
    }
}

/// The name in `locale`, else in [`FALLBACK_LOCALE`], else the first one the family has.
fn localized_name(names: &IDWriteLocalizedStrings, locale: &[u16]) -> Option<String> {
    let fallback: Vec<u16> = FALLBACK_LOCALE.encode_utf16().chain(Some(0)).collect();
    let index = [locale, fallback.as_slice()]
        .into_iter()
        .find_map(|wanted| {
            let (mut index, mut exists) = (0, BOOL::default());
            // SAFETY: `wanted` is NUL-terminated and outlives the call; `index` and `exists` are live locals.
            unsafe { names.FindLocaleName(PCWSTR(wanted.as_ptr()), &raw mut index, &raw mut exists) }.ok()?;
            exists.as_bool().then_some(index)
        })
        .unwrap_or(0);
    // SAFETY: the strings are alive.
    if unsafe { names.GetCount() } == 0 {
        return None;
    }
    // SAFETY: `index` is a name of this family, or 0 of a family that has at least one.
    let length = usize::try_from(unsafe { names.GetStringLength(index) }.ok()?).ok()?;
    let mut buffer = vec![0_u16; length + 1];
    // SAFETY: the buffer holds the name and its terminator.
    unsafe { names.GetString(index, &mut buffer) }.ok()?;
    buffer.truncate(length);
    Some(String::from_utf16_lossy(&buffer))
}

#[cfg(test)]
mod tests;
