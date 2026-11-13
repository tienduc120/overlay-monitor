//! A modeless dialog built from an in-memory template, the building block of the settings window. The template is
//! plain data, encoded to the `DLGTEMPLATE` byte layout without touching Windows, and the dialog reports what the user
//! did as [`UiEvent::Dialog`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ptr;
use std::rc::Rc;
use std::sync::Once;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_WINDOWFRAME, CreateSolidBrush, DeleteObject, DrawFocusRect, FillRect, FrameRect, GetSysColorBrush, HBRUSH,
    HGDIOBJ, InvalidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::{CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, HKCOMB_NONE, HKCOMB_S, HKM_GETHOTKEY, HKM_SETHOTKEY, HKM_SETRULES, HOTKEYF_ALT, HOTKEYF_CONTROL,
    HOTKEYF_SHIFT, ICC_HOTKEY_CLASS, ICC_UPDOWN_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx, ODS_FOCUS,
    UDM_GETBUDDY, UDM_GETRANGE32, UDM_SETBUDDY, UDM_SETPOS32, UDM_SETRANGE32, UDS_ALIGNRIGHT, UDS_ARROWKEYS,
    UDS_NOTHOUSANDS, UDS_SETBUDDYINT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_OWNERDRAW, BS_PUSHBUTTON,
    CB_ADDSTRING, CB_ERR, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CBN_SELCHANGE, CBS_DROPDOWNLIST,
    CreateDialogIndirectParamW, DS_CENTER, DS_MODALFRAME, DS_SETFONT, DestroyWindow, EN_CHANGE, ES_AUTOHSCROLL,
    ES_NUMBER, GWLP_USERDATA, GetDlgCtrlID, GetDlgItem, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
    IDC_HAND, IsIconic, LoadCursorW, SW_RESTORE, SW_SHOW, SendMessageW, SetCursor, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowTextW, ShowWindow, WINDOW_LONG_PTR_INDEX, WM_CLOSE, WM_COMMAND, WM_DRAWITEM,
    WM_INITDIALOG, WM_SETCURSOR, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL,
};
use windows::core::PCWSTR;

use crate::Win32Error;
use crate::hotkey::HotkeyKeys;
use crate::icon::WindowIcons;
use crate::ui::{EventSink, MessageWindow, Queue, UiEvent};

/// The id decorative items (labels, group boxes) carry, which Windows treats as "no control".
pub const DECORATIVE_ID: u16 = 0xFFFF;

/// The class atoms of the built-in controls.
const CLASS_BUTTON: u16 = 0x0080;
const CLASS_EDIT: u16 = 0x0081;
const CLASS_STATIC: u16 = 0x0082;
const CLASS_COMBO_BOX: u16 = 0x0085;
const CLASS_HOTKEY: &str = "msctls_hotkey32";
const CLASS_UPDOWN: &str = "msctls_updown32";

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
    /// A button that shows a colour set from code and reports its clicks, so Tab and Space reach it too.
    Swatch,
    /// Up and down arrows for the edit placed just before it, whole numbers from `min` to `max`; the edit shows the
    /// number and reports each change as its own.
    Spin {
        min: i16,
        max: i16,
    },
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
/// `DWLP_MSGRESULT`: where a dialog procedure leaves the answer to a message it handled, since its return value only
/// says it was handled.
const MESSAGE_RESULT: WINDOW_LONG_PTR_INDEX = WINDOW_LONG_PTR_INDEX(0);
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
        ItemKind::Swatch => (Class::Atom(CLASS_BUTTON), input | BS_OWNERDRAW as u32),
        // Not a tab stop: its edit is, and the arrow keys work from there.
        ItemKind::Spin { .. } => (
            Class::Name(CLASS_UPDOWN),
            child | UDS_SETBUDDYINT | UDS_ALIGNRIGHT | UDS_ARROWKEYS | UDS_NOTHOUSANDS,
        ),
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
    /// The colour picker's 16 custom colours, which Windows leaves to the caller to keep between pickers.
    custom_colours: Cell<[COLORREF; 16]>,
}

fn init_common_controls() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let classes = INITCOMMONCONTROLSEX {
            dwSize: u32::try_from(size_of::<INITCOMMONCONTROLSEX>()).unwrap_or(u32::MAX),
            dwICC: ICC_HOTKEY_CLASS | ICC_UPDOWN_CLASS,
        };
        // SAFETY: `classes` is a live local with its size set. A failure leaves the hotkey and arrow controls
        // unregistered, which the dialog's creation then reports.
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
        let dialog = Self {
            window,
            state,
            queue,
            // White, which is what the picker shows in an unused slot.
            custom_colours: Cell::new([COLORREF(0x00FF_FFFF); 16]),
        };
        dialog.queue.set_dialog(Some(window));
        // SAFETY: the window is alive; an invalid one would give 0, and the icon then comes out at the default size.
        if let Some(icons) = WindowIcons::of_this_exe(unsafe { GetDpiForWindow(window) }) {
            icons.apply(window);
        }
        for item in template.items.iter().filter(|item| item.kind == ItemKind::HotKey) {
            dialog.limit_hotkey_field(item.id);
        }
        for (before, item) in template.items.iter().zip(template.items.iter().skip(1)) {
            if let ItemKind::Spin { min, max } = item.kind {
                dialog.attach_arrows(item.id, before.id, (i32::from(min), i32::from(max)));
            }
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

    /// Makes `buddy` (an edit) the number the arrows `arrows` step, within `range`; the edit keeps the number it had.
    fn attach_arrows(&self, arrows: u16, buddy: u16, (min, max): (i32, i32)) {
        let (Some(spin), Some(edit)) = (self.control(arrows), self.control(buddy)) else {
            return;
        };
        let start = self.text(buddy).trim().parse::<i32>().unwrap_or(min).clamp(min, max);
        let edit_handle = edit.0.expose_provenance();
        // SAFETY: both controls are alive; the arguments are a range, the buddy's handle, and a position. Setting the
        // buddy rewrites its text from the position, so the start is set after it.
        self.quietly(|| unsafe {
            SendMessageW(
                spin,
                UDM_SETRANGE32,
                Some(WPARAM(min.cast_unsigned() as usize)),
                Some(LPARAM(max as isize)),
            );
            SendMessageW(spin, UDM_SETBUDDY, Some(WPARAM(edit_handle)), None);
            SendMessageW(spin, UDM_SETPOS32, None, Some(LPARAM(start as isize)));
        });
    }

    /// The id of the edit these arrows step, or `None` for a control that is not arrows or has no edit.
    pub fn spin_buddy(&self, id: u16) -> Option<u16> {
        let spin = self.control(id)?;
        // SAFETY: the control is alive; UDM_GETBUDDY takes no arguments.
        let buddy = unsafe { SendMessageW(spin, UDM_GETBUDDY, None, None) }.0;
        if buddy == 0 {
            return None;
        }
        let handle = HWND(ptr::with_exposed_provenance_mut(buddy.cast_unsigned()));
        // SAFETY: the handle is the window the arrows named.
        u16::try_from(unsafe { GetDlgCtrlID(handle) }).ok()
    }

    /// The smallest and largest number these arrows step between.
    pub fn spin_range(&self, id: u16) -> Option<(i32, i32)> {
        let spin = self.control(id)?;
        let (mut min, mut max) = (0_i32, 0_i32);
        // SAFETY: the control is alive; both pointers are live locals the call fills.
        unsafe {
            SendMessageW(
                spin,
                UDM_GETRANGE32,
                Some(WPARAM(ptr::from_mut(&mut min).expose_provenance())),
                Some(LPARAM(ptr::from_mut(&mut max).expose_provenance().cast_signed())),
            )
        };
        Some((min, max))
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
        let colour = COLORREF(colorref_from_rgb(rgb));
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

    pub fn is_enabled(&self, id: u16) -> bool {
        // SAFETY: the control is alive.
        self.control(id)
            .is_some_and(|control| unsafe { IsWindowEnabled(control) }.as_bool())
    }

    /// Opens Windows' colour picker at `current` (`0xRRGGBB`), over this dialog until it closes; `None` when the user
    /// cancels it. The custom colours the user mixes are kept for the next picker this dialog opens.
    pub fn choose_color(&self, current: u32) -> Option<u32> {
        let mut custom = self.custom_colours.get();
        let mut request = CHOOSECOLORW {
            lStructSize: u32::try_from(size_of::<CHOOSECOLORW>()).unwrap_or(u32::MAX),
            hwndOwner: self.window,
            rgbResult: COLORREF(colorref_from_rgb(current)),
            lpCustColors: custom.as_mut_ptr(),
            Flags: CC_RGBINIT | CC_FULLOPEN,
            ..CHOOSECOLORW::default()
        };
        // SAFETY: `request` and the 16 custom colours it points to are live locals for the whole call, which is modal
        // and returns only once the picker is closed.
        let chosen = unsafe { ChooseColorW(&raw mut request) }.as_bool();
        self.custom_colours.set(custom);
        chosen.then(|| rgb_from_colorref(request.rgbResult.0))
    }
}

/// Red and blue swap places: the settings store `0xRRGGBB`, Windows a `COLORREF` of `0x00BBGGRR`.
const fn swap_red_and_blue(colour: u32) -> u32 {
    ((colour & 0xFF) << 16) | (colour & 0xFF00) | ((colour >> 16) & 0xFF)
}

pub const fn colorref_from_rgb(rgb: u32) -> u32 {
    swap_red_and_blue(rgb)
}

pub const fn rgb_from_colorref(colorref: u32) -> u32 {
    swap_red_and_blue(colorref)
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
        WM_DRAWITEM => {
            // SAFETY: for this message `lparam` points to the `DRAWITEMSTRUCT` Windows built for the paint.
            let drawn = unsafe { ptr::with_exposed_provenance::<DRAWITEMSTRUCT>(lparam.0.cast_unsigned()).as_ref() };
            drawn.map_or(0, |item| {
                draw_swatch(state, item);
                1
            })
        }
        WM_SETCURSOR => isize::from(hand_over_swatch(state, window, wparam)),
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

/// The swatch a `WM_SETCURSOR` or draw message is about, by its control handle.
fn swatch_id(state: &State, control: HWND) -> Option<u16> {
    // SAFETY: the handle is the control Windows named in the message; a handle that is not a control gives 0.
    let id = u16::try_from(unsafe { GetDlgCtrlID(control) }).ok()?;
    state.swatches.borrow().contains_key(&id).then_some(id)
}

/// A swatch under the mouse shows the pointing hand, so it reads as clickable. True when the cursor was set.
fn hand_over_swatch(state: &State, window: HWND, wparam: WPARAM) -> bool {
    let control = HWND(ptr::with_exposed_provenance_mut(wparam.0));
    if swatch_id(state, control).is_none() {
        return false;
    }
    // SAFETY: the stock hand cursor is shared and needs no release; setting the cursor and the dialog's message result
    // have no other preconditions.
    unsafe {
        if let Ok(hand) = LoadCursorW(None, IDC_HAND) {
            SetCursor(Some(hand));
        }
        SetWindowLongPtrW(window, MESSAGE_RESULT, 1);
    }
    true
}

/// Paints a swatch: its colour filled, a frame, and a dotted rectangle while it has the keyboard.
fn draw_swatch(state: &State, item: &DRAWITEMSTRUCT) {
    let Ok(id) = u16::try_from(item.CtlID) else {
        return;
    };
    let Some(brush) = state.swatches.borrow().get(&id).map(|&(_, brush)| brush) else {
        return;
    };
    let inner = RECT {
        left: item.rcItem.left + 2,
        top: item.rcItem.top + 2,
        right: item.rcItem.right - 2,
        bottom: item.rcItem.bottom - 2,
    };
    // SAFETY: the device context and rectangle are the ones Windows passed for this paint, live for the call; the
    // frame's brush is a system colour brush, which is never freed.
    unsafe {
        FillRect(item.hDC, &raw const item.rcItem, brush);
        FrameRect(item.hDC, &raw const item.rcItem, GetSysColorBrush(COLOR_WINDOWFRAME));
        if item.itemState.0 & ODS_FOCUS.0 != 0 {
            DrawFocusRect(item.hDC, &raw const inner).as_bool();
        }
    }
}

#[cfg(test)]
mod tests;
