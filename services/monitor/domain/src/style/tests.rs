use super::{Rgb, Style};

#[test]
fn the_default_style_matches_the_screenshot() {
    let s = Style::default();
    assert_eq!(
        (s.font_family.as_str(), s.size_pt, s.bold, s.italic),
        ("Consolas", 14.0, true, false)
    );
    assert_eq!(
        (s.text, s.outline, s.outline_px),
        (Rgb(0x0000_FF00), Rgb(0x0000_0000), 2.0)
    );
    assert_eq!((s.background, s.background_opacity_percent), (Rgb(0x0000_0000), 0));
}

#[test]
fn an_invisible_background_still_catches_the_mouse_when_unlocked() {
    let s = Style::default();
    assert_eq!(
        s.background_alpha(false),
        1,
        "alpha 0 pixels let clicks through, so the overlay could not be dragged"
    );
    assert_eq!(s.background_alpha(true), 0);
}

#[test]
fn opacity_maps_to_alpha_rounding_half_up() {
    let half = Style {
        background_opacity_percent: 50,
        ..Style::default()
    };
    let full = Style {
        background_opacity_percent: 100,
        ..Style::default()
    };
    assert_eq!((half.background_alpha(true), full.background_alpha(true)), (128, 255));
}
