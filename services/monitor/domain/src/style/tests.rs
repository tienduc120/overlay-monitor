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

#[test]
fn colours_read_and_print_as_a_hash_and_six_hex_digits() {
    assert_eq!(Rgb::parse_hex("#00ff00"), Some(Rgb(0x0000_FF00)));
    assert_eq!(Rgb::parse_hex("00FF00"), Some(Rgb(0x0000_FF00)));
    assert_eq!(Rgb(0x0000_FF00).hex(), "#00FF00");
    assert_eq!(Rgb::parse_hex("#0F0"), None);
    assert_eq!(Rgb::parse_hex("#GG0000"), None);
}

#[test]
fn a_style_out_of_range_is_pulled_back() {
    let with = |f: fn(&mut Style)| {
        let mut s = Style::default();
        f(&mut s);
        s.clamped()
    };
    assert_eq!(with(|s| s.size_pt = 200.0).size_pt, 72.0);
    assert_eq!(with(|s| s.size_pt = 1.0).size_pt, 6.0);
    assert_eq!(with(|s| s.size_pt = f32::NAN).size_pt, 14.0);
    assert_eq!(with(|s| s.outline_px = -1.0).outline_px, 0.0);
    assert_eq!(with(|s| s.outline_px = 20.0).outline_px, 8.0);
    assert_eq!(
        with(|s| s.background_opacity_percent = 150).background_opacity_percent,
        100
    );
    assert_eq!(with(|s| s.font_family = "  ".into()).font_family, "Consolas");
}
