//! The settings window as plain state: what each field edit does to the edited settings, and what OK gives back.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use monitor_application::{Field, Line, Settings, ShortcutKeys, line_fields};
use platform_win32::dialog::FontChoice;

const SHORTEST_SECS: f64 = 0.25;
const LONGEST_SECS: f64 = 5.0;
const MAX_OUTLINE_PX: u8 = 8;
const MAX_OPACITY_PERCENT: u8 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    Text,
    Outline,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShortcutSlot {
    ShowHide,
    LockUnlock,
}

/// Why a field's text or choice cannot be used; the `Display` text is what the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Interval,
    OutlineWidth,
    Opacity,
    EmptyLabel,
    SameShortcut,
    NeedsCtrlOrAlt,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Interval => "Update every: enter seconds from 0.25 to 5.",
            Self::OutlineWidth => "Outline width: enter whole pixels from 0 to 8.",
            Self::Opacity => "Background opacity: enter a whole percent from 0 to 100.",
            Self::EmptyLabel => "Every label needs at least one character.",
            Self::SameShortcut => "Show/hide and lock/unlock need different shortcuts.",
            Self::NeedsCtrlOrAlt => "A shortcut needs Ctrl or Alt.",
        })
    }
}

/// What an edit did: the overlay should redraw, nothing moved, or the edit was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Preview,
    Unchanged,
    Invalid(Problem),
}

/// A field that can hold a problem; the order is the order `finish` reports them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Spot {
    Interval,
    OutlineWidth,
    Opacity,
    Label(Line),
    Shortcut(ShortcutSlot),
}

#[derive(Debug)]
pub struct SettingsEditor {
    original: Settings,
    edited: Settings,
    problems: BTreeMap<Spot, Problem>,
}

/// A whole number from 0 to `max`, nothing else.
fn whole(text: &str, max: u8) -> Option<u8> {
    text.trim().parse::<u8>().ok().filter(|n| *n <= max)
}

/// Seconds with `.` or `,` as the decimal mark, within the allowed range.
fn seconds(text: &str) -> Option<Duration> {
    let secs: f64 = text.trim().replace(',', ".").parse().ok()?;
    (SHORTEST_SECS..=LONGEST_SECS)
        .contains(&secs)
        .then(|| Duration::from_secs_f64(secs))
}

/// Stores `value` when it differs, and says whether the overlay needs a redraw.
fn store<T: PartialEq>(target: &mut T, value: T) -> Change {
    if *target == value {
        Change::Unchanged
    } else {
        *target = value;
        Change::Preview
    }
}

impl SettingsEditor {
    pub fn new(current: Settings) -> Self {
        Self {
            original: current.clone(),
            edited: current,
            problems: BTreeMap::new(),
        }
    }

    pub const fn edited(&self) -> &Settings {
        &self.edited
    }

    pub const fn original(&self) -> &Settings {
        &self.original
    }

    /// The first problem still standing, or the edited settings.
    ///
    /// # Errors
    /// The first field that holds an unfixed problem.
    pub fn finish(&self) -> Result<Settings, Problem> {
        self.problems
            .values()
            .next()
            .map_or_else(|| Ok(self.edited.clone()), |p| Err(*p))
    }

    /// A valid value clears the field's problem; none remembers `problem` and leaves the last good value.
    fn settle<T: PartialEq>(
        &mut self,
        spot: Spot,
        problem: Problem,
        value: Option<T>,
        pick: impl FnOnce(&mut Settings) -> &mut T,
    ) -> Change {
        let Some(value) = value else {
            self.problems.insert(spot, problem);
            return Change::Invalid(problem);
        };
        self.problems.remove(&spot);
        store(pick(&mut self.edited), value)
    }

    pub fn set_font(&mut self, font: &FontChoice) -> Change {
        let style = &mut self.edited.style;
        if style.font_family == font.family
            && style.size_pt.to_bits() == font.size_pt.to_bits()
            && style.bold == font.bold
            && style.italic == font.italic
        {
            return Change::Unchanged;
        }
        style.font_family.clone_from(&font.family);
        style.size_pt = font.size_pt;
        style.bold = font.bold;
        style.italic = font.italic;
        Change::Preview
    }

    pub fn set_colour(&mut self, which: Colour, rgb: u32) -> Change {
        let style = &mut self.edited.style;
        store(
            match which {
                Colour::Text => &mut style.text_rgb,
                Colour::Outline => &mut style.outline_rgb,
                Colour::Background => &mut style.background_rgb,
            },
            rgb,
        )
    }

    pub fn set_outline_width(&mut self, text: &str) -> Change {
        let px = whole(text, MAX_OUTLINE_PX).map(f32::from);
        self.settle(Spot::OutlineWidth, Problem::OutlineWidth, px, |s| {
            &mut s.style.outline_px
        })
    }

    pub fn set_opacity(&mut self, text: &str) -> Change {
        let percent = whole(text, MAX_OPACITY_PERCENT);
        self.settle(Spot::Opacity, Problem::Opacity, percent, |s| {
            &mut s.style.background_opacity_percent
        })
    }

    pub fn set_interval(&mut self, text: &str) -> Change {
        self.settle(Spot::Interval, Problem::Interval, seconds(text), |s| {
            &mut s.update_interval
        })
    }

    pub fn set_line_enabled(&mut self, line: Line, on: bool) -> Change {
        self.edited
            .lines
            .iter_mut()
            .find(|l| l.line == line)
            .map_or(Change::Unchanged, |l| store(&mut l.enabled, on))
    }

    /// Ticks or unticks one value of a line; `fields` stays in the line's display order.
    pub fn set_field(&mut self, line: Line, field: Field, on: bool) -> Change {
        if !line_fields(line).contains(&field) {
            return Change::Unchanged;
        }
        let Some(settings) = self.edited.lines.iter_mut().find(|l| l.line == line) else {
            return Change::Unchanged;
        };
        let fields = line_fields(line)
            .iter()
            .copied()
            .filter(|f| if *f == field { on } else { settings.fields.contains(f) })
            .collect();
        store(&mut settings.fields, fields)
    }

    pub fn set_label(&mut self, line: Line, text: &str) -> Change {
        let spot = Spot::Label(line);
        if text.trim().is_empty() {
            self.problems.insert(spot, Problem::EmptyLabel);
            return Change::Invalid(Problem::EmptyLabel);
        }
        self.problems.remove(&spot);
        self.edited
            .lines
            .iter_mut()
            .find(|l| l.line == line)
            .map_or(Change::Unchanged, |l| store(&mut l.label, String::from(text)))
    }

    /// `None` is automatic.
    pub fn set_gpu(&mut self, name: Option<String>) -> Change {
        store(&mut self.edited.gpu, name)
    }

    pub fn set_shortcut(&mut self, slot: ShortcutSlot, keys: ShortcutKeys) -> Change {
        let place = Spot::Shortcut(slot);
        let other = match slot {
            ShortcutSlot::ShowHide => self.edited.lock_unlock,
            ShortcutSlot::LockUnlock => self.edited.show_hide,
        };
        let refusal = if !keys.is_valid() {
            Some(Problem::NeedsCtrlOrAlt)
        } else if keys == other {
            Some(Problem::SameShortcut)
        } else {
            None
        };
        if let Some(problem) = refusal {
            self.problems.insert(place, problem);
            return Change::Invalid(problem);
        }
        self.problems.remove(&place);
        store(
            match slot {
                ShortcutSlot::ShowHide => &mut self.edited.show_hide,
                ShortcutSlot::LockUnlock => &mut self.edited.lock_unlock,
            },
            keys,
        )
    }

    pub fn set_start_with_windows(&mut self, on: bool) -> Change {
        store(&mut self.edited.start_with_windows, on)
    }
}

#[cfg(test)]
mod tests;
