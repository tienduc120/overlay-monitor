//! A modeless dialog built from an in-memory template, the building block of the settings window. The template is
//! plain data, encoded to the `DLGTEMPLATE` byte layout without touching Windows, and the dialog reports what the user
//! did as [`UiEvent::Dialog`](crate::ui::UiEvent::Dialog).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ptr;
use std::rc::Rc;
use std::sync::Once;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH, HGDIOBJ, InvalidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    HKCOMB_NONE, HKCOMB_S, HKM_GETHOTKEY, HKM_SETHOTKEY, HKM_SETRULES, HOTKEYF_ALT, HOTKEYF_CONTROL, HOTKEYF_SHIFT,
    ICC_HOTKEY_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_PUSHBUTTON, CB_ADDSTRING,
    CB_ERR, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CBN_SELCHANGE, CBS_DROPDOWNLIST, CreateDialogIndirectParamW,
    DS_CENTER, DS_MODALFRAME, DS_SETFONT, DestroyWindow, EN_CHANGE, ES_AUTOHSCROLL, ES_NUMBER, GWLP_USERDATA,
    GetDlgCtrlID, GetDlgItem, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsIconic, SW_RESTORE, SW_SHOW,
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW, ShowWindow, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLORSTATIC, WM_INITDIALOG, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_POPUP, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::PCWSTR;

use crate::Win32Error;
use crate::hotkey::HotkeyKeys;
use crate::ui::{EventSink, MessageWindow, Queue, UiEvent};

/// The id decorative items (labels, group boxes) carry, which Windows treats as "no control".
pub const DECORATIVE_ID: u16 = 0xFFFF;

/// The class atoms of the built-in controls.
const CLASS_BUTTON: u16 = 0x0080;
const CLASS_EDIT: u16 = 0x0081;
const CLASS_STATIC: u16 = 0x0082;
const CLASS_COMBO_BOX: u16 = 0x0085;
const CLASS_HOTKEY: &str = "msctls_hotkey32";

/// What an item of the dialog is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Label,
    GroupBox,
    CheckBox,
    Edit,
    /// An edit that takes digits only.
    NumberEdit,
    Button,
    /// The button Enter presses.
    DefaultButton,
    DropDownList,
    /// A field that records a key combination.
    HotKey,
    /// A bordered rectangle painted in a colour set from code.
    Swatch,
}

/// One control of the dialog; the position and size are in dialog units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogItem {
    /// What its events carry; [`DECORATIVE_ID`] for a label that is never read.
    pub id: u16,
    pub kind: ItemKind,
    pub text: String,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
}

/// A dialog's title, size in dialog units, and controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogTemplate {
    pub title: String,
    pub width: i16,
    pub height: i16,
    pub items: Vec<DialogItem>,
}

/// A font as the settings store it; plain data, the picker that fills it is separate.
#[derive(Clone, Debug, PartialEq)]
pub struct FontChoice {
    pub family: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
}

/// What the user did to a control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogChange {
    Clicked,
    Edited,
    Selected,
}

/// The dialog's own style bits, extended style, and font, as the brief of the settings window fixes them.
const DIALOG_STYLE: u32 =
    DS_SETFONT as u32 | DS_MODALFRAME as u32 | DS_CENTER as u32 | WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0;
/// The id of the Cancel button, which closing the window stands for.
const CANCEL_ID: u16 = 2;
const FONT_POINTS: u16 = 9;
const FONT_FAMILY: &str = "Segoe UI";

enum Class {
    Atom(u16),
    Name(&'static str),
}

fn item_class_and_style(kind: ItemKind) -> (Class, u32) {
    let child = WS_CHILD.0 | WS_VISIBLE.0;
    let input = child | WS_TABSTOP.0;
    match kind {
        ItemKind::Label => (Class::Atom(CLASS_STATIC), child),
        ItemKind::GroupBox => (Class::Atom(CLASS_BUTTON), child | BS_GROUPBOX as u32),
        ItemKind::CheckBox => (Class::Atom(CLASS_BUTTON), input | BS_AUTOCHECKBOX as u32),
        ItemKind::Edit => (Class::Atom(CLASS_EDIT), input | WS_BORDER.0 | ES_AUTOHSCROLL as u32),
        ItemKind::NumberEdit => (
            Class::Atom(CLASS_EDIT),
            input | WS_BORDER.0 | ES_AUTOHSCROLL as u32 | ES_NUMBER as u32,
        ),
        ItemKind::Button => (Class::Atom(CLASS_BUTTON), input | BS_PUSHBUTTON as u32),
        ItemKind::DefaultButton => (Class::Atom(CLASS_BUTTON), input | BS_DEFPUSHBUTTON as u32),
        ItemKind::DropDownList => (
            Class::Atom(CLASS_COMBO_BOX),
            input | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0,
        ),
        ItemKind::HotKey => (Class::Name(CLASS_HOTKEY), input | WS_BORDER.0),
        ItemKind::Swatch => (Class::Atom(CLASS_STATIC), child | WS_BORDER.0),
    }
}

/// The `DLGTEMPLATE` bytes for `template`: the header, then each item on a 4-byte boundary. The layout is Windows's,
/// so a mistake shows up as a dialog that fails to open or has the wrong controls.
pub fn encode_template(template: &DialogTemplate) -> Vec<u8> {
    let mut out = Encoder::default();
    out.u32(DIALOG_STYLE);
    out.u32(WS_EX_APPWINDOW.0);
    out.u16(u16::try_from(template.items.len()).unwrap_or(u16::MAX));
    out.i16(0);
    out.i16(0);
    out.i16(template.width);
    out.i16(template.height);
    out.u16(0); // no menu
    out.u16(0); // the default dialog class
    out.text(&template.title);
    out.u16(FONT_POINTS);
    out.text(FONT_FAMILY);
    for item in &template.items {
        out.align4();
        let (class, style) = item_class_and_style(item.kind);
        out.u32(style);
        out.u32(0);
        out.i16(item.x);
        out.i16(item.y);
        out.i16(item.width);
        out.i16(item.height);
        out.u16(item.id);
        match class {
            Class::Atom(atom) => {
                out.u16(0xFFFF);
                out.u16(atom);
            }
            Class::Name(name) => out.text(name),
        }
        out.text(&item.text);
        out.u16(0); // no creation data
    }
    out.bytes
}

#[derive(Default)]
struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i16(&mut self, value: i16) {
        self.u16(value.cast_unsigned());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn text(&mut self, text: &str) {
        for unit in text.encode_utf16().chain(Some(0)) {
            self.u16(unit);
        }
    }

    fn align4(&mut self) {
        let padded = self.bytes.len().next_multiple_of(4);
        self.bytes.resize(padded, 0);
    }
}

/// What the dialog procedure reads through the window's user data; it lives in a `Box` owned by [`Dialog`], so its
/// address is stable until the window is destroyed.
struct State {
    sink: EventSink,
    /// Set while code changes a control, so the notification that causes is not reported as the user's.
    quiet: Cell<bool>,
    /// The swatches' colours (as `COLORREF` values) and the brushes that paint them.
    swatches: RefCell<HashMap<u16, (u32, HBRUSH)>>,
}

/// An open modeless dialog; dropping it destroys the window.
pub struct Dialog {
    window: HWND,
    state: Box<State>,
    queue: Rc<Queue>,
}

fn init_common_controls() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let classes = INITCOMMONCONTROLSEX {
            dwSize: u32::try_from(size_of::<INITCOMMONCONTROLSEX>()).unwrap_or(u32::MAX),
            dwICC: ICC_HOTKEY_CLASS,
        };
        // SAFETY: `classes` is a live local with its size set. A failure leaves the hotkey field unregistered, which
        // the dialog's creation then reports.
        unsafe { InitCommonControlsEx(&raw const classes) }.as_bool();
    });
}

impl Dialog {
    /// Creates the dialog from `template`, shows it, and brings it to the front. The owner's loop delivers its events
    /// and passes its keys through `IsDialogMessageW`, so Tab, Enter, and Esc work.
    ///
    /// # Errors
    /// The window could not be created.
    pub fn open(owner: &MessageWindow, template: &DialogTemplate) -> Result<Self, Win32Error> {
        init_common_controls();
        // The template must be 4-byte aligned, which a `Vec<u8>` does not promise.
        let bytes = encode_template(template);
        let words: Vec<u32> = bytes
            .chunks(4)
            .map(|chunk| {
                let mut padded = [0_u8; 4];
                for (slot, byte) in padded.iter_mut().zip(chunk) {
                    *slot = *byte;
                }
                u32::from_le_bytes(padded)
            })
            .collect();
        let queue = owner.queue();
        let state = Box::new(State {
            sink: owner.sink(),
            quiet: Cell::new(false),
            swatches: RefCell::new(HashMap::new()),
        });
        // SAFETY: no name asks for this executable's own module.
        let instance = unsafe { GetModuleHandleW(None) }?;
        // SAFETY: `words` is a complete, aligned template that outlives the call; the owner is alive; the state is
        // boxed and outlives the window, because `Drop` destroys the window before the box is freed.
        let window = unsafe {
            CreateDialogIndirectParamW(
                Some(instance.into()),
                words.as_ptr().cast(),
                Some(owner.handle()),
                Some(dialog_procedure),
                LPARAM(ptr::from_ref::<State>(&state).expose_provenance().cast_signed()),
            )
        }?;
        let dialog = Self { window, state, queue };
        dialog.queue.set_dialog(Some(window));
        for item in template.items.iter().filter(|item| item.kind == ItemKind::HotKey) {
            dialog.limit_hotkey_field(item.id);
        }
        // SAFETY: the window is ours; the calls have no other preconditions.
        unsafe { ShowWindow(window, SW_SHOW) }.as_bool();
        dialog.bring_to_front();
        Ok(dialog)
    }

    /// A bare or Shift-only key becomes Ctrl+key, so a shortcut can never be typed into a text field by accident.
    fn limit_hotkey_field(&self, id: u16) {
        let Some(control) = self.control(id) else {
            return;
        };
        let invalid = usize::try_from(HKCOMB_NONE | HKCOMB_S).unwrap_or(0);
        let fallback = isize::try_from(HOTKEYF_CONTROL).unwrap_or(0);
        // SAFETY: the control is alive; the arguments are the invalid combinations and the modifier that replaces them.
        self.quietly(|| unsafe { SendMessageW(control, HKM_SETRULES, Some(WPARAM(invalid)), Some(LPARAM(fallback))) });
    }

    /// The control with this id, or `None` when the dialog has none.
    fn control(&self, id: u16) -> Option<HWND> {
        // SAFETY: the dialog window is alive; a missing id only makes the call fail.
        unsafe { GetDlgItem(Some(self.window), i32::from(id)) }.ok()
    }

    fn quietly<T>(&self, change: impl FnOnce() -> T) -> T {
        let before = self.state.quiet.replace(true);
        let result = change();
        self.state.quiet.set(before);
        result
    }

    /// What an edit or label shows, or the empty string for an unknown id.
    pub fn text(&self, id: u16) -> String {
        let Some(control) = self.control(id) else {
            return String::new();
        };
        // SAFETY: the control is alive.
        let length = usize::try_from(unsafe { GetWindowTextLengthW(control) }).unwrap_or(0);
        let mut buffer = vec![0_u16; length + 1];
        // SAFETY: the buffer holds the text and its terminator.
        let copied = usize::try_from(unsafe { GetWindowTextW(control, &mut buffer) }).unwrap_or(0);
        buffer
            .get(..copied.min(length))
            .map_or_else(String::new, String::from_utf16_lossy)
    }

    pub fn set_text(&self, id: u16, text: &str) {
        let Some(control) = self.control(id) else {
            return;
        };
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        // SAFETY: the control is alive and `wide` is NUL-terminated and outlives the call, which copies it.
        self.quietly(|| unsafe { SetWindowTextW(control, PCWSTR(wide.as_ptr())) }.ok());
    }

    pub fn is_checked(&self, id: u16) -> bool {
        let Some(control) = self.control(id) else {
            return false;
        };
        // SAFETY: the control is alive; BM_GETCHECK takes no arguments.
        unsafe { SendMessageW(control, BM_GETCHECK, None, None) }.0 == 1
    }

    pub fn set_checked(&self, id: u16, on: bool) {
        let Some(control) = self.control(id) else {
            return;
        };
        // SAFETY: the control is alive; the argument is a check state.
        self.quietly(|| unsafe { SendMessageW(control, BM_SETCHECK, Some(WPARAM(usize::from(on))), None) });
    }

    /// Replaces the drop-down's entries and selects the one at `selected`; an index past the end selects none.
    pub fn set_choices(&self, id: u16, choices: &[String], selected: usize) {
        let Some(control) = self.control(id) else {
            return;
        };
        self.quietly(|| {
            // SAFETY: the control is alive; each string is NUL-terminated and outlives its call, which copies it.
            unsafe { SendMessageW(control, CB_RESETCONTENT, None, None) };
            for choice in choices {
                let wide: Vec<u16> = choice.encode_utf16().chain(Some(0)).collect();
                // SAFETY: as above.
                unsafe {
                    SendMessageW(
                        control,
                        CB_ADDSTRING,
                        None,
                        Some(LPARAM(wide.as_ptr().expose_provenance().cast_signed())),
                    )
                };
            }
            // SAFETY: the control is alive.
            unsafe { SendMessageW(control, CB_SETCURSEL, Some(WPARAM(selected)), None) };
        });
    }

    /// The selected entry's index, or `None` when nothing is selected.
    pub fn choice(&self, id: u16) -> Option<usize> {
        let control = self.control(id)?;
        // SAFETY: the control is alive; CB_GETCURSEL takes no arguments.
        let index = unsafe { SendMessageW(control, CB_GETCURSEL, None, None) }.0;
        if index == CB_ERR as isize {
            None
        } else {
            usize::try_from(index).ok()
        }
    }

    /// The combination a hotkey field holds; all zero when it is empty.
    pub fn hotkey(&self, id: u16) -> HotkeyKeys {
        let packed = self.control(id).map_or(0, |control| {
            // SAFETY: the control is alive; HKM_GETHOTKEY takes no arguments.
            unsafe { SendMessageW(control, HKM_GETHOTKEY, None, None) }.0
        });
        let key = u8::try_from(packed & 0xFF).unwrap_or(0);
        let modifiers = u8::try_from((packed >> 8) & 0xFF).unwrap_or(0);
        HotkeyKeys {
            ctrl: u32::from(modifiers) & HOTKEYF_CONTROL != 0,
            shift: u32::from(modifiers) & HOTKEYF_SHIFT != 0,
            alt: u32::from(modifiers) & HOTKEYF_ALT != 0,
            key,
        }
    }

    pub fn set_hotkey(&self, id: u16, keys: HotkeyKeys) {
        let Some(control) = self.control(id) else {
            return;
        };
        let modifiers = (u32::from(keys.ctrl) * HOTKEYF_CONTROL)
            | (u32::from(keys.shift) * HOTKEYF_SHIFT)
            | (u32::from(keys.alt) * HOTKEYF_ALT);
        let packed = (modifiers << 8) | u32::from(keys.key);
        // SAFETY: the control is alive; the argument is a packed key and modifiers.
        self.quietly(|| unsafe { SendMessageW(control, HKM_SETHOTKEY, Some(WPARAM(packed as usize)), None) });
    }

    /// Paints the swatch in `rgb` (`0xRRGGBB`).
    pub fn set_swatch(&self, id: u16, rgb: u32) {
        let Some(control) = self.control(id) else {
            return;
        };
        let colour = COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF));
        // SAFETY: no preconditions; the brush is kept in the state and deleted when replaced or dropped.
        let brush = unsafe { CreateSolidBrush(colour) };
        if let Some((_, old)) = self.state.swatches.borrow_mut().insert(id, (colour.0, brush)) {
            delete_brush(old);
        }
        // SAFETY: the control is alive; a null rectangle repaints all of it.
        unsafe { InvalidateRect(Some(control), None, true) }.as_bool();
    }

    pub fn set_enabled(&self, id: u16, on: bool) {
        if let Some(control) = self.control(id) {
            // SAFETY: the control is alive. The result is the previous state, which does not matter.
            unsafe { EnableWindow(control, on) }.as_bool();
        }
    }

    /// Shows the dialog above other windows and gives it the keyboard.
    pub fn bring_to_front(&self) {
        // SAFETY: the window is alive. Windows may refuse the foreground to a process that was not the last to get
        // input; the dialog is then still shown and has its taskbar button.
        unsafe {
            if IsIconic(self.window).as_bool() {
                ShowWindow(self.window, SW_RESTORE).as_bool();
            }
            SetForegroundWindow(self.window).as_bool();
        }
    }
}

fn delete_brush(brush: HBRUSH) {
    // SAFETY: the brush was created here and is deleted once; a failure leaks only the brush.
    unsafe { DeleteObject(HGDIOBJ(brush.0)) }.as_bool();
}

impl Drop for Dialog {
    fn drop(&mut self) {
        self.queue.set_dialog(None);
        // SAFETY: the window was created on this thread, which owns it, and is destroyed once, here, before the
        // state its procedure reads is freed. A window already gone (its owner was destroyed first) only fails.
        unsafe { DestroyWindow(self.window) }.ok();
        for (_, (_, brush)) in self.state.swatches.borrow_mut().drain() {
            delete_brush(brush);
        }
    }
}

/// Turns the user's actions on the dialog into events; everything else is left to the default dialog handling.
///
/// # Safety
/// Only Windows calls this, with the arguments of a message for a dialog created by [`Dialog::open`].
unsafe extern "system" fn dialog_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if message == WM_INITDIALOG {
        // SAFETY: `lparam` is the state pointer `open` passed, which outlives the window.
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, lparam.0) };
        return 1;
    }
    // SAFETY: reading a window's user data has no preconditions.
    let data = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    let state = ptr::with_exposed_provenance::<State>(data.cast_unsigned());
    // SAFETY: non-zero data is the pointer `WM_INITDIALOG` stored; `Dialog` keeps the state alive until it has
    // destroyed the window, and only shared references are made from it. Messages sent while the controls are created
    // come first and find 0.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    match message {
        WM_COMMAND => {
            command(state, wparam);
            1
        }
        WM_CLOSE => {
            report(state, CANCEL_ID, DialogChange::Clicked);
            // Handled: the dialog is closed by dropping it, not by Windows.
            1
        }
        WM_CTLCOLORSTATIC => swatch_brush(state, lparam).unwrap_or(0),
        _ => 0,
    }
}

fn command(state: &State, wparam: WPARAM) {
    let id = u16::try_from(wparam.0 & 0xFFFF).unwrap_or(0);
    let code = u32::try_from((wparam.0 >> 16) & 0xFFFF).unwrap_or(0);
    let change = match code {
        BN_CLICKED => DialogChange::Clicked,
        EN_CHANGE => DialogChange::Edited,
        CBN_SELCHANGE => DialogChange::Selected,
        _ => return,
    };
    report(state, id, change);
}

fn report(state: &State, control: u16, change: DialogChange) {
    if !state.quiet.get() {
        state.sink.send(UiEvent::Dialog { control, change });
    }
}

/// The brush of the swatch `control` is, as the answer to `WM_CTLCOLORSTATIC`.
fn swatch_brush(state: &State, control: LPARAM) -> Option<isize> {
    let control = HWND(ptr::with_exposed_provenance_mut(control.0.cast_unsigned()));
    // SAFETY: the handle is the control Windows named in the message.
    let id = u16::try_from(unsafe { GetDlgCtrlID(control) }).ok()?;
    state
        .swatches
        .borrow()
        .get(&id)
        .map(|&(_, brush)| brush.0.expose_provenance().cast_signed())
}

#[cfg(test)]
mod tests;
