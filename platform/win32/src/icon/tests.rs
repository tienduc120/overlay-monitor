use windows::Win32::Foundation::{HMODULE, HWND, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, ICON_BIG, ICON_SMALL, SendMessageW, WM_GETICON, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};
use windows::core::w;

use crate::icon::{Icon, WindowIcons, icon_sizes, pick_ico_image};

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

/// A hidden top-level window of a built-in class, to set icons on and read them back.
fn hidden_window() -> HWND {
    // SAFETY: a stock class needs no registration; there is no parent, menu, or creation data.
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("icon test"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
    }
    .unwrap()
}

fn shell32() -> HMODULE {
    // SAFETY: the name is a static string; shell32 is loaded in every process that has a shell.
    unsafe { GetModuleHandleW(w!("shell32.dll")) }.unwrap()
}

/// An icon group `shell32.dll` is known to hold.
const SHELL32_ICON_GROUP: u16 = 1;

#[test]
fn the_big_icon_is_larger_than_the_small_one_at_every_dpi() {
    for dpi in [96, 144, 192] {
        let ((big_w, big_h), (small_w, small_h)) = icon_sizes(dpi);

        assert!(small_w >= 16 && small_h >= 16, "{dpi} dpi");
        assert!(big_w > small_w && big_h > small_h, "{dpi} dpi");
    }
}

#[test]
fn a_module_without_the_icon_gives_none() {
    assert!(WindowIcons::from_resource(shell32(), 60_000, 96).is_none());
}

#[test]
fn a_window_shows_the_icons_it_was_given() {
    let window = hidden_window();
    let icons = WindowIcons::from_resource(shell32(), SHELL32_ICON_GROUP, 96).expect("shell32 holds an icon group 1");

    icons.apply(window);

    // SAFETY: the window is alive and the messages only read its icons.
    let (big, small) = unsafe {
        (
            SendMessageW(window, WM_GETICON, Some(WPARAM(ICON_BIG as usize)), None),
            SendMessageW(window, WM_GETICON, Some(WPARAM(ICON_SMALL as usize)), None),
        )
    };
    assert_eq!(big.0, icons.big().0 as isize);
    assert_eq!(small.0, icons.small().0 as isize);
    assert_ne!(big.0, small.0, "two sizes, two handles");
    // SAFETY: the window was made here and is destroyed once.
    unsafe { DestroyWindow(window) }.unwrap();
}
