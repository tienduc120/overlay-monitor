use crate::icon::{Icon, pick_ico_image};

const OWNER_ICON: &[u8] = include_bytes!("../../../../services/monitor/api/assets/icon.ico");

/// The width of a PNG image, from its IHDR chunk.
fn png_width(image: &[u8]) -> u32 {
    assert_eq!(&image[..8], b"\x89PNG\r\n\x1a\n", "the image must be a PNG");
    u32::from_be_bytes(image[16..20].try_into().unwrap())
}

#[test]
fn the_smallest_image_at_least_as_wide_as_wanted_is_picked() {
    let picked = [(16, 16), (20, 24), (40, 48), (300, 256)]
        .map(|(wanted, _)| png_width(pick_ico_image(OWNER_ICON, wanted).expect("the owner's file has images")));

    assert_eq!(picked, [16, 24, 48, 256], "wanted 16, 20, 40, 300 px");
}

#[test]
fn a_truncated_file_has_no_image() {
    let truncated = &OWNER_ICON[..OWNER_ICON.len() / 2];

    assert_eq!(pick_ico_image(truncated, 16), None);
}

#[test]
fn a_cursor_file_is_not_an_icon() {
    let mut cursor = OWNER_ICON.to_vec();
    cursor[2] = 2;

    assert_eq!(pick_ico_image(&cursor, 16), None);
}

#[test]
fn an_empty_file_has_no_image() {
    assert_eq!(pick_ico_image(&[], 16), None);
}

#[test]
fn an_icon_is_made_at_the_small_and_the_medium_size() {
    Icon::from_ico(OWNER_ICON, 16).unwrap();
    Icon::from_ico(OWNER_ICON, 32).unwrap();
}
