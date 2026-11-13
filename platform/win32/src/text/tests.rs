use super::{TextPainter, TextStyle, installed_font_families, oversample, paint};
use crate::coverage::Coverage;

fn style(family: &str) -> TextStyle {
    TextStyle {
        font_family: family.to_owned(),
        size_pt: 14.0,
        bold: false,
        italic: false,
        text_rgb: 0x00FF_FFFF,
        outline_rgb: 0,
        outline_px: 1.0,
        background_rgb: 0,
        background_alpha: 0,
    }
}

#[test]
fn the_installed_families_include_the_ones_every_windows_has() {
    let families = installed_font_families().unwrap();

    assert!(families.iter().any(|name| name == "Segoe UI"), "Segoe UI is missing");
    assert!(families.iter().any(|name| name == "Consolas"), "Consolas is missing");
    assert!(families.iter().all(|name| !name.is_empty()), "a family has no name");
}

#[test]
fn every_family_listed_is_one_the_overlay_can_draw() {
    let painter = TextPainter::new().unwrap();
    let lines = ["Ab 12".to_owned()];

    for family in installed_font_families().unwrap() {
        let drawn = painter
            .lay_out(&lines, &style(&family), 16.0, (0.0, 0.0))
            .and_then(|text| painter.coverage(&text, 64, 32));

        assert!(drawn.is_ok(), "the overlay cannot draw {family:?}: {:?}", drawn.err());
    }
}

/// The alpha of each pixel of a `width` by `height` canvas once `line` is painted in Consolas at 16 px per em, its
/// text box's top-left at (4, 4), without an outline; and the text box.
fn painted_alpha(line: &str, width: usize, height: usize) -> (Vec<u8>, (f32, f32)) {
    let painter = TextPainter::new().unwrap();
    let style = TextStyle {
        outline_px: 0.0,
        ..style("Consolas")
    };
    let text = painter.lay_out(&[line.to_owned()], &style, 16.0, (4.0, 4.0)).unwrap();
    let fill = painter.coverage(&text, width, height).unwrap();
    let mut pixels = vec![[0_u8; 4]; width * height];
    paint(&mut pixels, &fill, &style);
    (pixels.iter().map(|pixel| pixel[3]).collect(), (text.width, text.height))
}

#[test]
fn letters_are_drawn_solid_inside_with_soft_edges_and_spaces_not_at_all() {
    let (letters, _) = painted_alpha("Ab", 48, 32);
    let (spaces, _) = painted_alpha("   ", 48, 32);

    assert!(letters.contains(&255), "a letter's inside must be fully covered");
    assert!(
        letters.iter().any(|&alpha| alpha > 0 && alpha < 255),
        "its edges must be anti-aliased"
    );
    assert!(spaces.iter().all(|&alpha| alpha == 0), "a space must cover nothing");
}

#[test]
fn the_glyphs_land_inside_their_text_box() {
    let (width, height) = (48, 32);
    let (alpha, (text_w, text_h)) = painted_alpha("Ab", width, height);

    let inked: Vec<(f32, f32)> = alpha
        .iter()
        .enumerate()
        .filter(|&(_, &alpha)| alpha > 0)
        .map(|(index, _)| {
            let (x, y) = (index % width, index / width);
            (
                f32::from(u16::try_from(x).unwrap()),
                f32::from(u16::try_from(y).unwrap()),
            )
        })
        .collect();

    assert!(!inked.is_empty(), "something must be drawn");
    // A pixel of slack for the anti-aliased edge; the box starts at (4, 4).
    for (x, y) in inked {
        assert!(
            (3.0..=4.0 + text_w).contains(&x) && (3.0..=4.0 + text_h).contains(&y),
            "ink at ({x}, {y}) lies outside the {text_w} x {text_h} box at (4, 4)"
        );
    }
}

#[test]
fn the_outline_goes_under_the_glyphs_and_reaches_past_their_edge() {
    let mut fill = Coverage::new(5, 1);
    fill.add(&[255], 1, (2, 0));
    let mut pixels = [[0, 0, 0, 1]; 5];
    let style = TextStyle {
        text_rgb: 0x00_FF_00,
        outline_rgb: 0x00_00_00,
        outline_px: 1.0,
        ..style("Consolas")
    };

    paint(&mut pixels, &fill, &style);

    assert_eq!(
        pixels,
        [
            [0, 0, 0, 1],
            [0, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 0, 255],
            [0, 0, 0, 1]
        ],
        "outline, glyph, outline, on a see-through background"
    );
}

#[test]
fn without_an_outline_only_the_glyphs_are_painted() {
    let mut fill = Coverage::new(3, 1);
    fill.add(&[255], 1, (1, 0));
    let mut pixels = [[0, 0, 0, 1]; 3];
    let style = TextStyle {
        text_rgb: 0x00_FF_00,
        outline_px: 0.0,
        ..style("Consolas")
    };

    paint(&mut pixels, &fill, &style);

    assert_eq!(pixels, [[0, 0, 0, 1], [0, 255, 0, 255], [0, 0, 0, 1]]);
}

#[test]
fn small_text_is_rasterized_four_times_finer_and_large_text_less() {
    assert_eq!(oversample(18.7), 4, "14 pt at 100 %");
    assert_eq!(oversample(64.0), 4, "the largest em still four times finer");
    assert_eq!(oversample(100.0), 2);
    assert_eq!(oversample(120.0), 2, "72 pt at 125 %");
    assert_eq!(oversample(300.0), 1, "never coarser than the canvas");
}
