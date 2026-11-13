//! The settings window: a dialog over the [`SettingsEditor`], turning what the user does to each control into a
//! preview for the overlay, a problem to show, or the end of the edit.

use std::time::Duration;

use monitor_application::{Field, Line, PawnIoStatus, Settings, ShortcutKeys, StyleSettings};
use platform_win32::Win32Error;
use platform_win32::dialog::{DECORATIVE_ID, Dialog, DialogChange, DialogItem, DialogTemplate, ItemKind};
use platform_win32::text::installed_font_families;
use platform_win32::ui::MessageWindow;

use crate::settings_editor::{Change, Colour, SettingsEditor, ShortcutSlot};
use crate::shortcuts::{hotkey_keys, shortcut_keys};

const OK: u16 = 1;
const CANCEL: u16 = 2;
const FONT_FAMILY: u16 = 101;
const FONT_SIZE: u16 = 102;
const SIZE_ARROWS: u16 = 111;
const BOLD: u16 = 112;
const ITALIC: u16 = 113;
/// The font size limits, in points, which the editor enforces too.
const SMALLEST_FONT_PT: i16 = 6;
const LARGEST_FONT_PT: i16 = 72;
const OUTLINE_WIDTH: u16 = 106;
const OPACITY: u16 = 109;
const GPU: u16 = 220;
const SHOW_HIDE_KEYS: u16 = 301;
const LOCK_UNLOCK_KEYS: u16 = 302;
const START_WITH_WINDOWS: u16 = 303;
const DRIVER_STATUS: u16 = 304;
const INSTALL_DRIVER: u16 = 305;
const INTERVAL: u16 = 221;
const PROBLEM: u16 = 400;
/// `PawnIO`'s own page, where its installer is.
pub(crate) const PAWNIO_URL: &str = "https://pawnio.eu/";
const AUTOMATIC_GPU: &str = "Automatic (most video memory)";

/// A colour and the swatch that shows it and opens its picker when clicked.
struct ColourRow {
    colour: Colour,
    swatch: u16,
}

static COLOUR_ROWS: [ColourRow; 3] = [
    ColourRow {
        colour: Colour::Text,
        swatch: 103,
    },
    ColourRow {
        colour: Colour::Outline,
        swatch: 105,
    },
    ColourRow {
        colour: Colour::Background,
        swatch: 108,
    },
];

/// A line's check box and label edit.
struct LineRow {
    line: Line,
    check: u16,
    label: u16,
    name: &'static str,
}

static LINE_ROWS: [LineRow; 4] = [
    LineRow {
        line: Line::Fps,
        check: 201,
        label: 211,
        name: "FPS",
    },
    LineRow {
        line: Line::Cpu,
        check: 202,
        label: 212,
        name: "CPU",
    },
    LineRow {
        line: Line::Gpu,
        check: 203,
        label: 213,
        name: "GPU",
    },
    LineRow {
        line: Line::Memory,
        check: 204,
        label: 214,
        name: "Memory",
    },
];

/// One value's check box, in its line's display order, and its width in dialog units.
struct ValueBox {
    id: u16,
    line: Line,
    field: Field,
    text: &'static str,
    width: i16,
}

const fn value(id: u16, line: Line, field: Field, text: &'static str, width: i16) -> ValueBox {
    ValueBox {
        id,
        line,
        field,
        text,
        width,
    }
}

static VALUE_BOXES: [ValueBox; 14] = [
    value(231, Line::Fps, Field::Fps, "FPS", 28),
    value(232, Line::Fps, Field::FrameTime, "frame time", 48),
    value(233, Line::Fps, Field::WorstFrameTime, "worst frame", 52),
    value(241, Line::Cpu, Field::Load, "load", 28),
    value(242, Line::Cpu, Field::Temperature, "temperature", 52),
    value(251, Line::Gpu, Field::Load, "load", 28),
    value(252, Line::Gpu, Field::Temperature, "temperature", 52),
    value(253, Line::Gpu, Field::Fan, "fan", 24),
    value(254, Line::Gpu, Field::MemoryClock, "VRAM clock", 50),
    value(255, Line::Gpu, Field::VideoLoad, "video", 30),
    value(261, Line::Memory, Field::Load, "load", 28),
    value(262, Line::Memory, Field::Used, "used", 28),
    value(263, Line::Memory, Field::Temperature, "temperature", 52),
    value(264, Line::Memory, Field::Sticks, "each stick", 48),
];

/// What the Ui does after an event of the window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WindowOutcome {
    Nothing,
    /// Show these settings now; they are kept only on OK.
    Preview(Settings),
    /// OK: keep these settings, and close.
    Finished(Settings),
    /// Cancel or close: go back to the settings the window opened with, and close.
    Cancelled(Settings),
    /// Install…: open `PawnIO`'s page.
    InstallPawnio,
}

/// The open settings window; dropping it closes the window.
pub(crate) struct SettingsWindow {
    dialog: Dialog,
    editor: SettingsEditor,
    /// The adapters the drop-down lists after "Automatic", in its order.
    gpu_names: Vec<String>,
    /// The family drop-down's entries, in its order.
    families: Vec<String>,
    /// Opens the colour picker at a colour; a field so a test can answer without the modal picker.
    pick_colour: fn(&Dialog, u32) -> Option<u32>,
}

impl SettingsWindow {
    /// Opens the window in front, its controls showing `settings`.
    ///
    /// # Errors
    /// The dialog could not be created.
    pub(crate) fn open(
        window: &MessageWindow,
        settings: &Settings,
        gpu_names: &[String],
        pawnio: &PawnIoStatus,
    ) -> Result<Self, Win32Error> {
        let opened = Self {
            dialog: Dialog::open(window, &settings_template())?,
            editor: SettingsEditor::new(settings.clone()),
            gpu_names: gpu_names.to_vec(),
            families: font_family_list(
                &installed_font_families().unwrap_or_default(),
                &settings.style.font_family,
            ),
            pick_colour: Dialog::choose_color,
        };
        opened.fill(settings);
        opened.show_driver(pawnio);
        Ok(opened)
    }

    fn fill(&self, settings: &Settings) {
        let style = &settings.style;
        let family = style.font_family.to_lowercase();
        let selected = self
            .families
            .iter()
            .position(|name| name.to_lowercase() == family)
            .unwrap_or_default();
        self.dialog.set_choices(FONT_FAMILY, &self.families, selected);
        self.dialog.set_text(FONT_SIZE, &size_text(style.size_pt));
        self.dialog.set_checked(BOLD, style.bold);
        self.dialog.set_checked(ITALIC, style.italic);
        for row in &COLOUR_ROWS {
            self.dialog.set_swatch(row.swatch, colour_of(style, row.colour));
        }
        self.dialog.set_text(OUTLINE_WIDTH, &format!("{:.0}", style.outline_px));
        self.dialog
            .set_text(OPACITY, &style.background_opacity_percent.to_string());
        for row in &LINE_ROWS {
            let Some(line) = settings.lines.iter().find(|l| l.line == row.line) else {
                continue;
            };
            self.dialog.set_checked(row.check, line.enabled);
            self.dialog.set_text(row.label, &line.label);
            for value in VALUE_BOXES.iter().filter(|v| v.line == row.line) {
                self.dialog.set_checked(value.id, line.fields.contains(&value.field));
            }
            self.enable_values(row.line, line.enabled);
        }
        let choices: Vec<String> = std::iter::once(AUTOMATIC_GPU.to_owned())
            .chain(self.gpu_names.iter().cloned())
            .collect();
        // A saved adapter that is gone shows as automatic, which is what the sampler falls back to.
        let selected = settings
            .gpu
            .as_ref()
            .and_then(|saved| self.gpu_names.iter().position(|name| name == saved))
            .map_or(0, |index| index + 1);
        self.dialog.set_choices(GPU, &choices, selected);
        self.dialog.set_text(INTERVAL, &seconds_text(settings.update_interval));
        self.dialog.set_hotkey(SHOW_HIDE_KEYS, hotkey_keys(settings.show_hide));
        self.dialog
            .set_hotkey(LOCK_UNLOCK_KEYS, hotkey_keys(settings.lock_unlock));
        self.dialog.set_checked(START_WITH_WINDOWS, settings.start_with_windows);
    }

    fn show_driver(&self, status: &PawnIoStatus) {
        self.dialog
            .set_text(DRIVER_STATUS, &format!("Sensor driver (PawnIO): {}", status.describe()));
        self.dialog.set_enabled(INSTALL_DRIVER, offers_install(status));
    }

    /// Reads the control the user changed and hands it to the editor.
    pub(crate) fn on_event(&mut self, control: u16, change: DialogChange) -> WindowOutcome {
        let edit = match (control, change) {
            (OK, DialogChange::Clicked) => return self.finish(),
            (CANCEL, DialogChange::Clicked) => return WindowOutcome::Cancelled(self.editor.original().clone()),
            (INSTALL_DRIVER, DialogChange::Clicked) => return WindowOutcome::InstallPawnio,
            (SHOW_HIDE_KEYS | LOCK_UNLOCK_KEYS, DialogChange::Edited) => self.edited_shortcut(control),
            (FONT_FAMILY, DialogChange::Selected) => self.choose_family(),
            (FONT_SIZE, DialogChange::Edited) => self.editor.set_size(&self.dialog.text(control)),
            (OUTLINE_WIDTH, DialogChange::Edited) => self.editor.set_outline_width(&self.dialog.text(control)),
            (OPACITY, DialogChange::Edited) => self.editor.set_opacity(&self.dialog.text(control)),
            (INTERVAL, DialogChange::Edited) => self.editor.set_interval(&self.dialog.text(control)),
            (GPU, DialogChange::Selected) => self.choose_gpu(),
            (_, DialogChange::Clicked) => self.clicked(control),
            (_, DialogChange::Edited) => self.edited_label(control),
            (_, DialogChange::Selected) => Change::Unchanged,
        };
        self.outcome(edit)
    }

    /// The settings the window opened with, which stay the saved ones until OK.
    pub(crate) const fn original(&self) -> &Settings {
        self.editor.original()
    }

    pub(crate) fn show_problem(&self, text: &str) {
        self.dialog.set_text(PROBLEM, text);
    }

    pub(crate) fn bring_to_front(&self) {
        self.dialog.bring_to_front();
    }

    #[cfg(test)]
    pub(crate) const fn dialog(&self) -> &Dialog {
        &self.dialog
    }

    fn finish(&self) -> WindowOutcome {
        match self.editor.finish() {
            Ok(settings) => WindowOutcome::Finished(settings),
            Err(problem) => {
                self.show_problem(&problem.to_string());
                WindowOutcome::Nothing
            }
        }
    }

    /// A refused edit shows why; any other shows the first problem still standing, or clears the line.
    fn outcome(&self, change: Change) -> WindowOutcome {
        if let Change::Invalid(problem) = change {
            self.show_problem(&problem.to_string());
            return WindowOutcome::Nothing;
        }
        let standing = self.editor.finish().err().map(|problem| problem.to_string());
        self.show_problem(standing.as_deref().unwrap_or_default());
        match change {
            Change::Preview => WindowOutcome::Preview(self.editor.edited().clone()),
            Change::Unchanged | Change::Invalid(_) => WindowOutcome::Nothing,
        }
    }

    /// A colour's swatch, bold or italic, a line's check box, or a value's check box.
    fn clicked(&mut self, control: u16) -> Change {
        if let Some(row) = COLOUR_ROWS.iter().find(|row| row.swatch == control) {
            return self.choose_colour(row);
        }
        let on = self.dialog.is_checked(control);
        match control {
            BOLD => return self.editor.set_bold(on),
            ITALIC => return self.editor.set_italic(on),
            _ => {}
        }
        if control == START_WITH_WINDOWS {
            return self.editor.set_start_with_windows(on);
        }
        if let Some(row) = LINE_ROWS.iter().find(|row| row.check == control) {
            self.enable_values(row.line, on);
            return self.editor.set_line_enabled(row.line, on);
        }
        VALUE_BOXES
            .iter()
            .find(|value| value.id == control)
            .map_or(Change::Unchanged, |value| {
                self.editor.set_field(value.line, value.field, on)
            })
    }

    fn edited_shortcut(&mut self, control: u16) -> Change {
        let slot = if control == SHOW_HIDE_KEYS {
            ShortcutSlot::ShowHide
        } else {
            ShortcutSlot::LockUnlock
        };
        self.editor
            .set_shortcut(slot, shortcut_keys(self.dialog.hotkey(control)))
    }

    fn edited_label(&mut self, control: u16) -> Change {
        LINE_ROWS
            .iter()
            .find(|row| row.label == control)
            .map_or(Change::Unchanged, |row| {
                self.editor.set_label(row.line, &self.dialog.text(control))
            })
    }

    /// Puts `keys` back in a shortcut's field and in the editor, after the shortcut could not be registered.
    pub(crate) fn restore_shortcut(&mut self, slot: ShortcutSlot, keys: ShortcutKeys) {
        let field = match slot {
            ShortcutSlot::ShowHide => SHOW_HIDE_KEYS,
            ShortcutSlot::LockUnlock => LOCK_UNLOCK_KEYS,
        };
        self.dialog.set_hotkey(field, hotkey_keys(keys));
        self.editor.set_shortcut(slot, keys);
    }

    /// Puts the check box and the editor back to `on`, after the start with Windows could not be changed.
    pub(crate) fn restore_start_with_windows(&mut self, on: bool) {
        self.dialog.set_checked(START_WITH_WINDOWS, on);
        self.editor.set_start_with_windows(on);
    }

    fn choose_gpu(&mut self) -> Change {
        let name = self
            .dialog
            .choice(GPU)
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| self.gpu_names.get(index))
            .cloned();
        self.editor.set_gpu(name)
    }

    fn choose_family(&mut self) -> Change {
        let family = self
            .dialog
            .choice(FONT_FAMILY)
            .and_then(|index| self.families.get(index));
        family.map_or(Change::Unchanged, |name| self.editor.set_family(name))
    }

    fn choose_colour(&mut self, row: &ColourRow) -> Change {
        let current = colour_of(&self.editor.edited().style, row.colour);
        let Some(rgb) = (self.pick_colour)(&self.dialog, current) else {
            return Change::Unchanged;
        };
        self.dialog.set_swatch(row.swatch, rgb);
        self.editor.set_colour(row.colour, rgb)
    }

    /// A line that is off shows none of its values, so they cannot be ticked meanwhile.
    fn enable_values(&self, line: Line, on: bool) {
        for value in VALUE_BOXES.iter().filter(|value| value.line == line) {
            self.dialog.set_enabled(value.id, on);
        }
    }
}

const fn colour_of(style: &StyleSettings, which: Colour) -> u32 {
    match which {
        Colour::Text => style.text_rgb,
        Colour::Outline => style.outline_rgb,
        Colour::Background => style.background_rgb,
    }
}

/// Whether the window offers the Install… button: `PawnIO` is missing, too old, or did not answer. When it is there
/// but needs admin, installing again would not help.
pub(crate) const fn offers_install(status: &PawnIoStatus) -> bool {
    matches!(
        status,
        PawnIoStatus::NotInstalled | PawnIoStatus::TooOld { .. } | PawnIoStatus::Unanswered { .. }
    )
}

/// The size as its box shows it: "14", and a saved fraction such as "10.5" as it is.
pub(crate) fn size_text(size_pt: f32) -> String {
    size_pt.to_string()
}

/// The family drop-down's entries: the installed families and the saved one, sorted, each once. A saved family that is
/// not installed stays in the list, so it stays selected and visible; the overlay then falls back as it does today.
pub(crate) fn font_family_list(installed: &[String], saved: &str) -> Vec<String> {
    let mut list = installed.to_vec();
    if !list.iter().any(|name| name.to_lowercase() == saved.to_lowercase()) {
        list.push(saved.to_owned());
    }
    list.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
    list.dedup_by(|later, earlier| later.to_lowercase() == earlier.to_lowercase());
    list
}

/// Seconds with two decimals for a quarter, one otherwise, so a whole second still reads as "1.0".
pub(crate) fn seconds_text(interval: Duration) -> String {
    let text = format!("{:.2}", interval.as_secs_f64());
    if let Some(shorter) = text.strip_suffix('0') {
        return shorter.to_owned();
    }
    text
}

/// `edited`, but shown or hidden, locked or not, and placed as the overlay is now: those change outside the window
/// while it is open, and the window must not undo them. The shortcuts and the start with Windows are not previews: they
/// take effect on OK, once Windows has accepted them.
pub(crate) fn keep_live_view_state(edited: Settings, live: &Settings) -> Settings {
    Settings {
        visible: live.visible,
        locked: live.locked,
        show_hide: live.show_hide,
        lock_unlock: live.lock_unlock,
        start_with_windows: live.start_with_windows,
        position: live.position.clone(),
        ..edited
    }
}

/// One control; positions and sizes are dialog units, as `(x, y, width, height)`.
fn item(id: u16, kind: ItemKind, text: &str, (x, y, width, height): (i16, i16, i16, i16)) -> DialogItem {
    DialogItem {
        id,
        kind,
        text: text.to_owned(),
        x,
        y,
        width,
        height,
    }
}

fn label(text: &str, place: (i16, i16, i16, i16)) -> DialogItem {
    item(DECORATIVE_ID, ItemKind::Label, text, place)
}

const DIALOG_WIDTH: i16 = 340;
const MARGIN: i16 = 7;
const GROUP_WIDTH: i16 = DIALOG_WIDTH - 2 * MARGIN;
/// Where a group's controls start, and where its buttons end.
const INSIDE_LEFT: i16 = MARGIN + 7;
const INSIDE_RIGHT: i16 = MARGIN + GROUP_WIDTH - 8;
const BUTTON_WIDTH: i16 = 50;
const BUTTON_HEIGHT: i16 = 14;
const STYLE_TOP: i16 = 4;
const STYLE_HEIGHT: i16 = 86;
const READINGS_TOP: i16 = STYLE_TOP + STYLE_HEIGHT + 4;
const READINGS_HEIGHT: i16 = 116;
const GENERAL_TOP: i16 = READINGS_TOP + READINGS_HEIGHT + 4;
/// Room for the shortcuts, start with Windows, and the sensor driver's state.
const GENERAL_HEIGHT: i16 = 78;
const PROBLEM_TOP: i16 = GENERAL_TOP + GENERAL_HEIGHT + 6;
const BUTTONS_TOP: i16 = PROBLEM_TOP + 16;
const DIALOG_HEIGHT: i16 = BUTTONS_TOP + BUTTON_HEIGHT + MARGIN;

/// The window's controls, in the mockup's order, which is also the Tab order.
pub(crate) fn settings_template() -> DialogTemplate {
    let mut items = style_items();
    items.extend(readings_items());
    items.extend(general_items());
    items.push(item(
        PROBLEM,
        ItemKind::Label,
        "",
        (MARGIN, PROBLEM_TOP, GROUP_WIDTH, 10),
    ));
    let cancel_left = INSIDE_RIGHT - BUTTON_WIDTH;
    let ok_left = cancel_left - BUTTON_WIDTH - 4;
    items.push(item(
        OK,
        ItemKind::DefaultButton,
        "OK",
        (ok_left, BUTTONS_TOP, BUTTON_WIDTH, BUTTON_HEIGHT),
    ));
    items.push(item(
        CANCEL,
        ItemKind::Button,
        "Cancel",
        (cancel_left, BUTTONS_TOP, BUTTON_WIDTH, BUTTON_HEIGHT),
    ));
    DialogTemplate {
        title: "Overlay Monitor settings".to_owned(),
        width: DIALOG_WIDTH,
        height: DIALOG_HEIGHT,
        items,
    }
}

/// The two shortcuts, start with Windows, and the sensor driver's state with its Install… button.
fn general_items() -> Vec<DialogItem> {
    const FIELD_LEFT: i16 = INSIDE_LEFT + 52;
    let [show_hide, lock_unlock, start, driver] = [12, 28, 44, 60].map(|offset: i16| GENERAL_TOP + offset);
    vec![
        item(
            DECORATIVE_ID,
            ItemKind::GroupBox,
            "General",
            (MARGIN, GENERAL_TOP, GROUP_WIDTH, GENERAL_HEIGHT),
        ),
        label("Show/hide", (INSIDE_LEFT, show_hide + 2, 50, 8)),
        item(SHOW_HIDE_KEYS, ItemKind::HotKey, "", (FIELD_LEFT, show_hide, 110, 12)),
        label("Lock/unlock", (INSIDE_LEFT, lock_unlock + 2, 50, 8)),
        item(
            LOCK_UNLOCK_KEYS,
            ItemKind::HotKey,
            "",
            (FIELD_LEFT, lock_unlock, 110, 12),
        ),
        item(
            START_WITH_WINDOWS,
            ItemKind::CheckBox,
            "Start with Windows",
            (INSIDE_LEFT, start, 120, 10),
        ),
        item(
            DRIVER_STATUS,
            ItemKind::Label,
            "",
            (INSIDE_LEFT, driver, INSIDE_RIGHT - BUTTON_WIDTH - 6 - INSIDE_LEFT, 16),
        ),
        item(
            INSTALL_DRIVER,
            ItemKind::Button,
            "Install\u{2026}",
            (INSIDE_RIGHT - BUTTON_WIDTH, driver, BUTTON_WIDTH, BUTTON_HEIGHT),
        ),
    ]
}

/// The font row, then the three colours, each a swatch that opens its picker when clicked.
fn style_items() -> Vec<DialogItem> {
    const VALUE_LEFT: i16 = INSIDE_LEFT + 48;
    const SWATCH: (i16, i16) = (28, 12);
    const WORD_LEFT: i16 = VALUE_LEFT + SWATCH.0 + 8;
    const NUMBER_LEFT: i16 = WORD_LEFT + 30;
    const UNIT_LEFT: i16 = NUMBER_LEFT + 28;
    const FAMILY_WIDTH: i16 = 110;
    const SIZE_WORD_LEFT: i16 = VALUE_LEFT + FAMILY_WIDTH + 6;
    const SIZE_LEFT: i16 = SIZE_WORD_LEFT + 20;
    const SIZE_WIDTH: i16 = 30;
    const POINTS_LEFT: i16 = SIZE_LEFT + SIZE_WIDTH + 4;
    const BOLD_LEFT: i16 = POINTS_LEFT + 14;
    const ITALIC_LEFT: i16 = BOLD_LEFT + 34;
    let rows: [i16; 4] = [16, 34, 52, 70].map(|offset| STYLE_TOP + offset);
    let [font, text, outline, background] = rows;
    let swatch = |id: u16, top: i16| item(id, ItemKind::Swatch, "", (VALUE_LEFT, top + 1, SWATCH.0, SWATCH.1));
    let number = |id: u16, top: i16| item(id, ItemKind::NumberEdit, "", (NUMBER_LEFT, top + 1, 24, 12));
    vec![
        item(
            DECORATIVE_ID,
            ItemKind::GroupBox,
            "Style",
            (MARGIN, STYLE_TOP, GROUP_WIDTH, STYLE_HEIGHT),
        ),
        label("Font:", (INSIDE_LEFT, font + 3, 46, 8)),
        item(
            FONT_FAMILY,
            ItemKind::DropDownList,
            "",
            (VALUE_LEFT, font, FAMILY_WIDTH, 140),
        ),
        label("Size", (SIZE_WORD_LEFT, font + 3, 18, 8)),
        item(
            FONT_SIZE,
            ItemKind::NumberEdit,
            "",
            (SIZE_LEFT, font + 1, SIZE_WIDTH, 12),
        ),
        item(
            SIZE_ARROWS,
            ItemKind::Spin {
                min: SMALLEST_FONT_PT,
                max: LARGEST_FONT_PT,
            },
            "",
            (SIZE_LEFT, font + 1, SIZE_WIDTH, 12),
        ),
        label("pt", (POINTS_LEFT, font + 3, 12, 8)),
        item(BOLD, ItemKind::CheckBox, "Bold", (BOLD_LEFT, font + 1, 32, 10)),
        item(ITALIC, ItemKind::CheckBox, "Italic", (ITALIC_LEFT, font + 1, 34, 10)),
        label("Text:", (INSIDE_LEFT, text + 3, 46, 8)),
        swatch(103, text),
        label("Outline:", (INSIDE_LEFT, outline + 3, 46, 8)),
        swatch(105, outline),
        label("width", (WORD_LEFT, outline + 3, 28, 8)),
        number(OUTLINE_WIDTH, outline),
        label("px", (UNIT_LEFT, outline + 3, 14, 8)),
        label("Background:", (INSIDE_LEFT, background + 3, 46, 8)),
        swatch(108, background),
        label("opacity", (WORD_LEFT, background + 3, 28, 8)),
        number(OPACITY, background),
        label("%", (UNIT_LEFT, background + 3, 14, 8)),
    ]
}

/// A row per line, its values after its label; then the adapter and the update interval.
fn readings_items() -> Vec<DialogItem> {
    const LABEL_EDIT_LEFT: i16 = INSIDE_LEFT + 66;
    const VALUES_LEFT: i16 = LABEL_EDIT_LEFT + 44;
    const ROW_STEP: i16 = 16;
    const FIRST_ROW: i16 = READINGS_TOP + 12;
    let adapter = FIRST_ROW + 4 * ROW_STEP + 2;
    let interval = adapter + 18;
    let mut items = vec![item(
        DECORATIVE_ID,
        ItemKind::GroupBox,
        "Readings",
        (MARGIN, READINGS_TOP, GROUP_WIDTH, READINGS_HEIGHT),
    )];
    for (row, top) in LINE_ROWS.iter().zip((0..).map(|n| FIRST_ROW + n * ROW_STEP)) {
        items.push(item(
            row.check,
            ItemKind::CheckBox,
            row.name,
            (INSIDE_LEFT, top + 1, 42, 10),
        ));
        items.push(label("label", (INSIDE_LEFT + 44, top + 2, 20, 8)));
        items.push(item(row.label, ItemKind::Edit, "", (LABEL_EDIT_LEFT, top, 40, 12)));
        let mut left = VALUES_LEFT;
        for value in VALUE_BOXES.iter().filter(|value| value.line == row.line) {
            items.push(item(
                value.id,
                ItemKind::CheckBox,
                value.text,
                (left, top + 1, value.width, 10),
            ));
            left += value.width + 4;
        }
    }
    items.extend([
        label("GPU adapter", (INSIDE_LEFT, adapter + 2, 44, 8)),
        item(
            GPU,
            ItemKind::DropDownList,
            "",
            (LABEL_EDIT_LEFT - 18, adapter, 160, 80),
        ),
        label("Update every", (INSIDE_LEFT, interval + 2, 44, 8)),
        item(INTERVAL, ItemKind::Edit, "", (LABEL_EDIT_LEFT - 18, interval, 28, 12)),
        label("s", (LABEL_EDIT_LEFT + 14, interval + 2, 10, 8)),
    ]);
    items
}

#[cfg(test)]
mod tests;
