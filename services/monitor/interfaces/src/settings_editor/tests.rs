use std::time::Duration;

use monitor_application::{Field, Line, Settings, ShortcutKeys};

use super::{Change, Colour, Problem, SettingsEditor, ShortcutSlot};

fn editor() -> SettingsEditor {
    SettingsEditor::new(Settings::default())
}

fn fields_of(editor: &SettingsEditor, line: Line) -> Vec<Field> {
    editor
        .edited()
        .lines
        .iter()
        .find(|l| l.line == line)
        .map(|l| l.fields.clone())
        .unwrap_or_default()
}

#[test]
fn a_new_colour_previews_at_once() {
    let mut e = editor();

    assert_eq!(e.set_colour(Colour::Text, 0x00FF_00FF), Change::Preview);
    assert_eq!(e.edited().style.text_rgb, 0x00FF_00FF);
}

#[test]
fn the_same_value_again_is_unchanged() {
    let mut e = editor();
    let same = e.edited().style.outline_rgb;

    assert_eq!(e.set_colour(Colour::Outline, same), Change::Unchanged);
    assert_eq!(e.set_opacity("40"), Change::Preview);
    assert_eq!(e.set_opacity("40"), Change::Unchanged);
}

#[test]
fn an_unparsable_interval_keeps_the_last_good_value_and_says_why() {
    let mut e = editor();
    assert_eq!(e.set_interval("2"), Change::Preview);

    assert_eq!(e.set_interval("abc"), Change::Invalid(Problem::Interval));

    assert_eq!(e.edited().update_interval, Duration::from_secs(2));
    assert_eq!(e.finish().map(|_| ()), Err(Problem::Interval));
}

#[test]
fn an_interval_out_of_range_is_a_problem_not_a_clamp() {
    let mut e = editor();
    let before = e.edited().update_interval;

    assert_eq!(e.set_interval("10"), Change::Invalid(Problem::Interval));
    assert_eq!(e.set_interval("NaN"), Change::Invalid(Problem::Interval));
    assert_eq!(e.edited().update_interval, before);
}

#[test]
fn interval_accepts_a_comma_as_the_decimal_mark() {
    let mut e = editor();

    e.set_interval("0,5");

    assert_eq!(e.edited().update_interval, Duration::from_millis(500));
}

#[test]
fn an_empty_label_is_a_problem() {
    let mut e = editor();

    assert_eq!(e.set_label(Line::Cpu, "  "), Change::Invalid(Problem::EmptyLabel));
    assert_eq!(e.set_label(Line::Cpu, "PROC"), Change::Preview);
}

#[test]
fn the_same_shortcut_for_both_is_refused() {
    let mut e = editor();
    let lock = e.edited().lock_unlock;
    let no_modifier = ShortcutKeys {
        ctrl: false,
        shift: false,
        alt: false,
        key: b'O',
    };

    assert_eq!(
        e.set_shortcut(ShortcutSlot::ShowHide, lock),
        Change::Invalid(Problem::SameShortcut)
    );
    assert_eq!(
        e.set_shortcut(ShortcutSlot::ShowHide, no_modifier),
        Change::Invalid(Problem::NeedsCtrlOrAlt)
    );
}

fn ctrl_shift(key: u8) -> ShortcutKeys {
    ShortcutKeys {
        ctrl: true,
        shift: true,
        alt: false,
        key,
    }
}

#[test]
fn a_key_a_shortcut_cannot_use_is_named_as_the_problem() {
    let mut e = editor();

    for key in [0x20, 0x24, 0x60] {
        // Space, Home, and numpad 0, which the field accepts.
        assert_eq!(
            e.set_shortcut(ShortcutSlot::ShowHide, ctrl_shift(key)),
            Change::Invalid(Problem::UnsupportedKey),
            "{key:#x}"
        );
    }
    assert_eq!(
        Problem::UnsupportedKey.to_string(),
        "A shortcut is A\u{2013}Z, 0\u{2013}9, or F1\u{2013}F24 with Ctrl or Alt."
    );
}

#[test]
fn fixing_the_other_side_of_a_clash_lets_ok_finish() {
    let mut e = editor();
    assert_eq!(
        e.set_shortcut(ShortcutSlot::ShowHide, ctrl_shift(b'L')),
        Change::Invalid(Problem::SameShortcut)
    );

    e.set_shortcut(ShortcutSlot::LockUnlock, ctrl_shift(b'K'));

    assert_eq!(
        e.finish().map(|s| (s.show_hide, s.lock_unlock)),
        Ok((ctrl_shift(b'L'), ctrl_shift(b'K')))
    );
}

#[test]
fn a_clash_is_judged_between_what_both_fields_hold() {
    let mut e = editor();
    e.set_shortcut(ShortcutSlot::ShowHide, ctrl_shift(b'L'));

    // Show/hide's field holds L and the edited settings still hold O; the fields differ, so nothing clashes.
    e.set_shortcut(ShortcutSlot::LockUnlock, ctrl_shift(b'O'));

    assert_eq!(
        e.finish().map(|s| (s.show_hide, s.lock_unlock)),
        Ok((ctrl_shift(b'L'), ctrl_shift(b'O')))
    );
}

#[test]
fn a_problem_left_unfixed_refuses_finish() {
    let mut e = editor();
    e.set_outline_width("9");

    assert_eq!(e.finish().map(|_| ()), Err(Problem::OutlineWidth));
}

#[test]
fn fixing_the_field_clears_its_problem() {
    let mut e = editor();
    e.set_opacity("101");
    assert_eq!(e.finish().map(|_| ()), Err(Problem::Opacity));

    e.set_opacity("100");

    assert_eq!(e.finish().map(|s| s.style.background_opacity_percent), Ok(100));
}

#[test]
fn choosing_automatic_clears_the_gpu_name() {
    let mut e = editor();
    assert_eq!(e.set_gpu(Some(String::from("RTX"))), Change::Preview);

    assert_eq!(e.set_gpu(None), Change::Preview);

    assert_eq!(e.edited().gpu, None);
}

#[test]
fn ticking_a_value_previews_it_in_the_lines_order() {
    let mut e = editor();

    assert_eq!(e.set_field(Line::Gpu, Field::VideoLoad, true), Change::Preview);
    assert_eq!(e.set_field(Line::Gpu, Field::Fan, true), Change::Preview);

    assert_eq!(
        fields_of(&e, Line::Gpu),
        [Field::Load, Field::Temperature, Field::Fan, Field::VideoLoad]
    );
}

#[test]
fn a_value_the_line_cannot_show_is_unchanged() {
    let mut e = editor();

    assert_eq!(e.set_field(Line::Cpu, Field::Fan, true), Change::Unchanged);
}

#[test]
fn the_original_is_what_the_editor_started_with() {
    let mut e = editor();
    e.set_colour(Colour::Background, 1);
    e.set_interval("3");
    e.set_line_enabled(Line::Fps, false);
    e.set_start_with_windows(true);

    assert_eq!(e.original(), &Settings::default());
}
