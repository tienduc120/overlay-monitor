//! Outlined text: each line's glyphs as Direct2D geometry, stroked in the outline colour and then filled in the text
//! colour, so it reads on any background. It draws on any Direct2D render target, whatever presents it.

use windows::Win32::Foundation::{DWRITE_E_NOFONT, E_INVALIDARG, E_POINTER};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_LINE_JOIN_ROUND,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1CreateFactory, ID2D1Brush, ID2D1Factory, ID2D1PathGeometry, ID2D1RenderTarget,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_GLYPH_METRICS,
    DWRITE_GLYPH_OFFSET, DWriteCreateFactory, IDWriteFactory, IDWriteFontFace,
};
use windows::core::{BOOL, HSTRING};

use crate::Win32Error;

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

/// Turns lines of text into glyph outlines and paints them. Direct2D makes and draws the geometry; DirectWrite
/// supplies the glyphs.
pub(crate) struct TextPainter {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    /// Round joins keep the outline from spiking at the sharp corners of glyphs.
    round: ID2D1StrokeStyle,
}

/// Lines of text as glyph outlines, already in place on the target.
pub(crate) struct OutlinedText {
    lines: Vec<ID2D1PathGeometry>,
    /// The text's size in pixels, without the outline.
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// One line's place on the target: where its glyphs start, and its baseline.
struct Placement {
    left: f32,
    baseline: f32,
}

impl TextPainter {
    pub(crate) fn new() -> Result<Self, Win32Error> {
        // SAFETY: no options are passed; the factory is only used from the thread that owns this painter.
        let d2d: ID2D1Factory = unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        // SAFETY: the call takes only the factory type.
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        let properties = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 1.0,
            dashStyle: D2D1_DASH_STYLE_SOLID,
            dashOffset: 0.0,
        };
        // SAFETY: `properties` is a live local; a solid stroke has no dashes.
        let round = unsafe { d2d.CreateStrokeStyle(&raw const properties, None) }?;
        Ok(Self { d2d, dwrite, round })
    }

    /// The factory a render target must come from to draw this painter's geometry.
    pub(crate) fn factory(&self) -> &ID2D1Factory {
        &self.d2d
    }

    /// Outlines `lines` in `style`'s font at `em_px` pixels per em, with the text's top-left corner at
    /// (`left`, `top`).
    ///
    /// # Errors
    /// `DWRITE_E_NOFONT` when the font family is not installed; otherwise a DirectWrite or Direct2D failure.
    pub(crate) fn outline(
        &self,
        lines: &[String],
        style: &TextStyle,
        em_px: f32,
        (left, top): (f32, f32),
    ) -> Result<OutlinedText, Win32Error> {
        let face = self.font_face(style)?;
        let mut metrics = DWRITE_FONT_METRICS::default();
        // SAFETY: `metrics` is a live local the call fills.
        unsafe { face.GetMetrics(&raw mut metrics) };
        let px_per_unit = em_px / f32::from(metrics.designUnitsPerEm);
        let ascent = f32::from(metrics.ascent) * px_per_unit;
        let descent = f32::from(metrics.descent) * px_per_unit;
        let line_advance = ascent + descent + f32::from(metrics.lineGap) * px_per_unit;

        let mut outlined = Vec::with_capacity(lines.len());
        let (mut width, mut bottom) = (0.0_f32, top);
        let mut baseline = top + ascent;
        for line in lines {
            // A whole-pixel baseline keeps every line's horizontal strokes equally sharp.
            let place = Placement {
                left,
                baseline: baseline.round(),
            };
            let (geometry, advance) = self.outline_line(&face, line, em_px, px_per_unit, &place)?;
            outlined.push(geometry);
            width = width.max(advance);
            bottom = baseline + descent;
            baseline += line_advance;
        }
        Ok(OutlinedText {
            lines: outlined,
            width,
            height: bottom - top,
        })
    }

    /// Strokes every line's outline at `2 * outline_px` with round joins, then fills the glyphs: the stroke is centred
    /// on the glyph's edge and the fill covers its inner half, so `outline_px` of outline shows. Call it between
    /// `BeginDraw` and `EndDraw` on a target made from [`TextPainter::factory`].
    pub(crate) fn paint(
        &self,
        target: &ID2D1RenderTarget,
        text: &OutlinedText,
        style: &TextStyle,
    ) -> Result<(), Win32Error> {
        let outline = solid_brush(target, style.outline_rgb)?;
        let fill = solid_brush(target, style.text_rgb)?;
        if style.outline_px > 0.0 {
            // Every outline first, so a line's outline never covers the letters of the line above it.
            for line in &text.lines {
                // SAFETY: the geometry, brush, and stroke style come from this target's factory and are alive.
                unsafe { target.DrawGeometry(line, &outline, 2.0 * style.outline_px, &self.round) };
            }
        }
        for line in &text.lines {
            // SAFETY: the geometry and brush come from this target's factory and are alive; no opacity mask.
            unsafe { target.FillGeometry(line, &fill, None::<&ID2D1Brush>) };
        }
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

    /// One line's glyph outlines, placed at `place`, and the line's advance width in pixels.
    fn outline_line(
        &self,
        face: &IDWriteFontFace,
        line: &str,
        em_px: f32,
        px_per_unit: f32,
        place: &Placement,
    ) -> Result<(ID2D1PathGeometry, f32), Win32Error> {
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
        let advances: Vec<f32> = metrics
            .iter()
            .map(|glyph| design_units(glyph.advanceWidth) * px_per_unit)
            .collect();
        // Every glyph moves by the same offset, so the outline lands in place and draws without a transform. A
        // positive ascender offset moves up, and the target's y axis points down.
        let offset = DWRITE_GLYPH_OFFSET {
            advanceOffset: place.left,
            ascenderOffset: -place.baseline,
        };
        let offsets = vec![offset; glyphs.len()];

        // SAFETY: the factory is alive.
        let geometry = unsafe { self.d2d.CreatePathGeometry() }?;
        // SAFETY: the geometry was just created and is opened once.
        let sink = unsafe { geometry.Open() }?;
        // SAFETY: the glyph, advance, and offset buffers hold `count` elements each, and the sink is open. DirectWrite's
        // `IDWriteGeometrySink` is Direct2D's simplified sink, which the geometry sink extends.
        unsafe {
            face.GetGlyphRunOutline(
                em_px,
                glyphs.as_ptr(),
                Some(advances.as_ptr()),
                Some(offsets.as_ptr()),
                count,
                false,
                false,
                &sink,
            )
        }?;
        // SAFETY: the sink is open, and is closed once.
        unsafe { sink.Close() }?;
        Ok((geometry, advances.iter().sum()))
    }
}

/// A glyph measure in font design units. OpenType stores advances as 16-bit values, which convert to f32 exactly.
fn design_units(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn solid_brush(target: &ID2D1RenderTarget, rgb: u32) -> Result<ID2D1SolidColorBrush, Win32Error> {
    let [_, red, green, blue] = rgb.to_be_bytes();
    let unit = |channel: u8| f32::from(channel) / 255.0;
    // Direct2D takes straight (not premultiplied) colours.
    let colour = D2D1_COLOR_F {
        r: unit(red),
        g: unit(green),
        b: unit(blue),
        a: 1.0,
    };
    // SAFETY: `colour` is a live local; the brush takes default properties.
    unsafe { target.CreateSolidColorBrush(&raw const colour, None) }
}
