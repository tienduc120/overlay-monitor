use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, GetDlgItem, PostMessageW, SendMessageW, WM_CLOSE, WM_COMMAND, WM_KEYDOWN,
};

use super::{
    Dialog, DialogChange, DialogItem, DialogTemplate, FontChoice, ItemKind, colorref_from_rgb, encode_template,
    font_from_logfont, logfont_height, rgb_from_colorref,
};
use crate::hotkey::HotkeyKeys;
use crate::ui::{Flow, MessageWindow, UiEvent, run_message_loop};

const CHECK: u16 = 10;
const EDIT: u16 = 11;
const NUMBER: u16 = 12;
const CHOICE: u16 = 13;
const KEYS: u16 = 14;
const OK: u16 = 1;
const CANCEL: u16 = 2;

fn item(id: u16, kind: ItemKind, text: &str, y: i16) -> DialogItem {
    DialogItem {
        id,
        kind,
        text: text.to_owned(),
        x: 8,
        y,
        width: 80,
        height: 12,
    }
}

fn template() -> DialogTemplate {
    DialogTemplate {
        title: "Test".to_owned(),
        width: 200,
        height: 140,
        items: vec![
            item(0xFFFF, ItemKind::Label, "Name", 4),
            item(CHECK, ItemKind::CheckBox, "On", 18),
            item(EDIT, ItemKind::Edit, "", 32),
            item(NUMBER, ItemKind::NumberEdit, "5", 46),
            item(CHOICE, ItemKind::DropDownList, "", 60),
            item(KEYS, ItemKind::HotKey, "", 74),
            item(OK, ItemKind::DefaultButton, "OK", 100),
            item(CANCEL, ItemKind::Button, "Cancel", 100),
        ],
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u16(&mut self) -> u16 {
        let value = u16::from_le_bytes([self.bytes[self.at], self.bytes[self.at + 1]]);
        self.at += 2;
        value
    }

    fn i16(&mut self) -> i16 {
        self.u16().cast_signed()
    }

    fn u32(&mut self) -> u32 {
        let low = u32::from(self.u16());
        low | (u32::from(self.u16()) << 16)
    }

    fn text(&mut self) -> String {
        let mut units = Vec::new();
        loop {
            match self.u16() {
                0 => break,
                unit => units.push(unit),
            }
        }
        String::from_utf16(&units).unwrap()
    }

    fn align4(&mut self) {
        self.at = self.at.next_multiple_of(4);
    }
}

struct DecodedItem {
    start: usize,
    style: u32,
    id: u16,
    class: Class,
    text: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Class {
    Atom(u16),
    Name(String),
}

fn decode_items(bytes: &[u8], count: u16) -> Vec<DecodedItem> {
    let mut reader = Reader { bytes, at: 0 };
    reader.u32();
    reader.u32();
    reader.u16();
    for _ in 0..4 {
        reader.u16();
    }
    reader.u16();
    reader.u16();
    reader.text();
    reader.u16();
    reader.text();
    let mut items = Vec::new();
    for _ in 0..count {
        reader.align4();
        let start = reader.at;
        let style = reader.u32();
        reader.u32();
        for _ in 0..4 {
            reader.u16();
        }
        let id = reader.u16();
        let class = if reader.u16() == 0xFFFF {
            Class::Atom(reader.u16())
        } else {
            reader.at -= 2;
            Class::Name(reader.text())
        };
        let text = reader.text();
        assert_eq!(reader.u16(), 0, "no creation data");
        items.push(DecodedItem {
            start,
            style,
            id,
            class,
            text,
        });
    }
    items
}

#[test]
fn the_template_starts_with_the_header_and_counts_its_items() {
    let bytes = encode_template(&template());
    let mut reader = Reader { bytes: &bytes, at: 0 };

    let style = reader.u32();
    let extended = reader.u32();
    let count = reader.u16();
    let (x, y, width, height) = (reader.i16(), reader.i16(), reader.i16(), reader.i16());
    let (menu, class) = (reader.u16(), reader.u16());
    let title = reader.text();
    let point_size = reader.u16();
    let font = reader.text();

    let expected = 0x0000_0040 // DS_SETFONT
        | 0x0000_0080 // DS_MODALFRAME
        | 0x0000_0800 // DS_CENTER
        | 0x8000_0000 // WS_POPUP
        | 0x00C0_0000 // WS_CAPTION
        | 0x0008_0000; // WS_SYSMENU
    assert_eq!(style, expected);
    assert_eq!(extended, 0x0004_0000, "WS_EX_APPWINDOW");
    assert_eq!(count, 8);
    assert_eq!((x, y, width, height), (0, 0, 200, 140));
    assert_eq!((menu, class), (0, 0));
    assert_eq!(title, "Test");
    assert_eq!(point_size, 9);
    assert_eq!(font, "Segoe UI");
}

#[test]
fn every_item_starts_on_a_four_byte_boundary() {
    let bytes = encode_template(&template());

    let items = decode_items(&bytes, 8);

    assert_eq!(items.len(), 8);
    for decoded in &items {
        assert_eq!(decoded.start % 4, 0, "item {} starts at {}", decoded.id, decoded.start);
    }
}

#[test]
fn standard_controls_use_class_atoms_and_the_hotkey_field_its_class_name() {
    let bytes = encode_template(&template());

    let items = decode_items(&bytes, 8);

    let find = |id: u16| items.iter().find(|decoded| decoded.id == id).unwrap();
    assert_eq!(find(0xFFFF).class, Class::Atom(0x0082));
    assert_eq!(find(CHECK).class, Class::Atom(0x0080));
    assert_eq!(find(EDIT).class, Class::Atom(0x0081));
    assert_eq!(find(CHOICE).class, Class::Atom(0x0085));
    assert_eq!(find(KEYS).class, Class::Name("msctls_hotkey32".to_owned()));
    assert_ne!(find(NUMBER).style & 0x2000, 0, "ES_NUMBER");
    assert_eq!(find(NUMBER).text, "5");
    for input in [CHECK, EDIT, NUMBER, CHOICE, KEYS, OK, CANCEL] {
        assert_ne!(find(input).style & 0x0001_0000, 0, "WS_TABSTOP on {input}");
    }
}

fn open_with(window: &MessageWindow) -> Dialog {
    Dialog::open(window, &template()).unwrap()
}

fn loop_events(window: &MessageWindow, exit_on: impl Fn(UiEvent) -> bool) -> Vec<UiEvent> {
    let mut seen = Vec::new();
    run_message_loop(window, &mut |event| {
        seen.push(event);
        if exit_on(event) { Flow::Exit } else { Flow::Continue }
    });
    seen
}

#[test]
fn values_set_from_code_read_back() {
    let window = MessageWindow::create().unwrap();
    let dialog = open_with(&window);
    let keys = HotkeyKeys {
        ctrl: true,
        shift: true,
        alt: false,
        key: b'O',
    };

    dialog.set_text(EDIT, "hello");
    dialog.set_checked(CHECK, true);
    dialog.set_choices(CHOICE, &["a".to_owned(), "b".to_owned(), "c".to_owned()], 1);
    dialog.set_hotkey(KEYS, keys);

    assert_eq!(dialog.text(EDIT), "hello");
    assert_eq!(dialog.text(NUMBER), "5");
    assert!(dialog.is_checked(CHECK));
    assert_eq!(dialog.choice(CHOICE), Some(1));
    assert_eq!(dialog.hotkey(KEYS), keys);
}

#[test]
fn setting_a_value_from_code_is_not_reported_as_a_change() {
    let window = MessageWindow::create().unwrap();
    let dialog = open_with(&window);

    dialog.set_text(EDIT, "typed by code");
    dialog.set_checked(CHECK, true);
    dialog.set_choices(CHOICE, &["a".to_owned(), "b".to_owned()], 0);
    window.sink().send(UiEvent::Wake);
    let seen = loop_events(&window, |event| event == UiEvent::Wake);

    assert_eq!(seen, [UiEvent::Wake]);
}

#[test]
fn a_click_reaches_the_loop() {
    let window = MessageWindow::create().unwrap();
    let dialog = open_with(&window);
    // SAFETY: the dialog is live.
    let control = unsafe { GetDlgItem(Some(dialog.window), i32::from(OK)) }.unwrap();
    let wparam = WPARAM((usize::try_from(BN_CLICKED).unwrap() << 16) | usize::from(OK));
    let lparam = LPARAM(control.0.expose_provenance().cast_signed());

    // SAFETY: the dialog is live; the message is the one a button click sends.
    unsafe { SendMessageW(dialog.window, WM_COMMAND, Some(wparam), Some(lparam)) };
    window.sink().send(UiEvent::Wake);
    let seen = loop_events(&window, |event| event == UiEvent::Wake);

    assert_eq!(
        seen,
        [
            UiEvent::Dialog {
                control: OK,
                change: DialogChange::Clicked
            },
            UiEvent::Wake
        ]
    );
}

#[test]
fn closing_the_dialog_is_a_cancel() {
    let window = MessageWindow::create().unwrap();
    let dialog = open_with(&window);

    // SAFETY: the dialog is live; WM_CLOSE carries no data.
    unsafe { SendMessageW(dialog.window, WM_CLOSE, None, None) };
    window.sink().send(UiEvent::Wake);
    let seen = loop_events(&window, |event| event == UiEvent::Wake);

    assert_eq!(
        seen,
        [
            UiEvent::Dialog {
                control: CANCEL,
                change: DialogChange::Clicked
            },
            UiEvent::Wake
        ]
    );
}

#[test]
fn escape_goes_through_the_loop_as_a_cancel() {
    let window = MessageWindow::create().unwrap();
    let dialog = open_with(&window);
    // SAFETY: the dialog is live.
    let edit = unsafe { GetDlgItem(Some(dialog.window), i32::from(EDIT)) }.unwrap();
    // SAFETY: the control is live; the message is a key press.
    unsafe { PostMessageW(Some(edit), WM_KEYDOWN, WPARAM(usize::from(VK_ESCAPE.0)), LPARAM(0)) }.unwrap();
    // A timer ends the loop if Escape never arrives, so a failure is a red test and not a hang.
    window.set_tick(Duration::from_millis(1500)).unwrap();

    let seen = loop_events(&window, |event| matches!(event, UiEvent::Dialog { .. } | UiEvent::Tick));

    assert_eq!(
        seen,
        [UiEvent::Dialog {
            control: CANCEL,
            change: DialogChange::Clicked
        }]
    );
}

#[test]
fn colours_swap_their_red_and_blue_bytes_for_colorref() {
    assert_eq!(colorref_from_rgb(0x00FF_8000), 0x0000_80FF);
    assert_eq!(rgb_from_colorref(0x0000_80FF), 0x00FF_8000);
}

#[test]
fn a_logfont_reads_as_a_font_choice() {
    let bold = font_from_logfont("Consolas", 700, false, 140);
    let regular = font_from_logfont("Consolas", 400, false, 140);

    assert_eq!(
        bold,
        FontChoice {
            family: "Consolas".to_owned(),
            size_pt: 14.0,
            bold: true,
            italic: false,
        }
    );
    assert!(!regular.bold, "weight 400 is regular");
}

#[test]
fn a_point_size_becomes_a_logfont_height_in_pixels() {
    assert_eq!(logfont_height(14.0, 96), -19, "14 pt at 96 dpi is 18.7 px, rounded");
    assert_eq!(logfont_height(9.0, 144), -18);
}

#[test]
fn a_disabled_control_reads_back_as_disabled() {
    let window = MessageWindow::create().unwrap();
    let dialog = open_with(&window);

    dialog.set_enabled(CHECK, false);

    assert!(!dialog.is_enabled(CHECK));
    assert!(dialog.is_enabled(EDIT));
}
