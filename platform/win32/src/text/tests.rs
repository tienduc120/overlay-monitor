use super::{TextPainter, TextStyle, installed_font_families};

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
        let drawn = painter.outline(&lines, &style(&family), 16.0, (0.0, 0.0));

        assert!(drawn.is_ok(), "the overlay cannot find {family:?}: {:?}", drawn.err());
    }
}
