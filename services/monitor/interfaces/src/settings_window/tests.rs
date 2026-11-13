use std::time::Duration;

use monitor_application::{
    Field, Line, LineSettings, OverlayPosition, PawnIoStatus, Settings, ShortcutKeys, StyleSettings,
};
use platform_win32::dialog::{DECORATIVE_ID, DialogChange, ItemKind};
use platform_win32::ui::{MessageWindow, pump_messages_for};

use crate::hotkey_keys;

use super::{
    PAWNIO_URL, SettingsWindow, WindowOutcome, font_text, keep_live_view_state, offers_install, seconds_text,
    settings_template,
};

fn gpu_names() -> Vec<String> {
    vec!["Radeon RX 9060 XT".to_owned(), "AMD Radeon(TM) Graphics".to_owned()]
}

fn open(window: &MessageWindow) -> SettingsWindow {
    SettingsWindow::open(window, &Settings::default(), &gpu_names(), &PawnIoStatus::NotInstalled).unwrap()
}

fn line_of(settings: &Settings, line: Line) -> &LineSettings {
    settings.lines.iter().find(|l| l.line == line).unwrap()
}

#[test]
fn every_control_id_appears_once() {
    let template = settings_template();

    let mut ids: Vec<u16> = template
        .items
        .iter()
        .map(|item| item.id)
        .filter(|id| *id != DECORATIVE_ID)
        .collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();

    assert_eq!(ids.len(), count, "an id is used twice");
    let mut expected: Vec<u16> = vec![1, 2, 220, 221, 400];
    expected.extend(101..=110);
    expected.extend(201..=204);
    expected.extend(301..=305);
    expected.extend(211..=214);
    expected.extend([231, 232, 233, 241, 242, 251, 252, 253, 254, 255, 261, 262, 263, 264]);
    expected.sort_unstable();
    assert_eq!(ids, expected, "every control the brief names, and no other");
}

#[test]
fn ok_is_the_default_button_and_cancel_is_id_2() {
    let template = settings_template();
    let find = |id: u16| template.items.iter().find(|item| item.id == id).unwrap();

    assert_eq!((find(1).kind, find(1).text.as_str()), (ItemKind::DefaultButton, "OK"));
    assert_eq!((find(2).kind, find(2).text.as_str()), (ItemKind::Button, "Cancel"));
    assert_eq!(template.title, "Overlay Monitor settings");
}

#[test]
fn the_font_line_reads_like_the_mockup() {
    let arial = StyleSettings {
        font_family: "Arial".to_owned(),
        size_pt: 10.5,
        bold: false,
        italic: true,
        ..Settings::default().style
    };

    assert_eq!(font_text(&Settings::default().style), "Consolas Bold, 14 pt");
    assert_eq!(font_text(&arial), "Arial Italic, 10.5 pt");
}

#[test]
fn seconds_print_with_one_or_two_decimals() {
    assert_eq!(seconds_text(Duration::from_secs(1)), "1.0");
    assert_eq!(seconds_text(Duration::from_millis(250)), "0.25");
    assert_eq!(seconds_text(Duration::from_millis(2500)), "2.5");
}

#[test]
fn opening_fills_the_controls_from_the_settings() {
    let window = MessageWindow::create().unwrap();

    let opened = open(&window);

    let dialog = &opened.dialog;
    for line in 201..=204 {
        assert!(dialog.is_checked(line), "line {line} is on by default");
    }
    let labels: Vec<String> = (211..=214).map(|id| dialog.text(id)).collect();
    assert_eq!(labels, ["FPS", "CPU", "GPU", "DDR5"]);
    assert_eq!(dialog.text(221), "1.0");
    assert_eq!(dialog.choice(220), Some(0), "automatic");
    assert_eq!(dialog.text(101), "Consolas Bold, 14 pt");
    assert_eq!((dialog.text(106).as_str(), dialog.text(109).as_str()), ("2", "0"));
    let values = [231, 232, 233, 241, 242, 251, 252, 253, 254, 255, 261, 262, 263, 264];
    let ticked: Vec<u16> = values.into_iter().filter(|id| dialog.is_checked(*id)).collect();
    assert_eq!(ticked, [231, 241, 242, 251, 252, 261, 263], "only today's values");
}

#[test]
fn a_saved_gpu_shows_as_its_choice_and_an_unknown_one_as_automatic() {
    let window = MessageWindow::create().unwrap();
    let second = Settings {
        gpu: Some("AMD Radeon(TM) Graphics".to_owned()),
        ..Settings::default()
    };
    let unknown = Settings {
        gpu: Some("A card that was removed".to_owned()),
        ..Settings::default()
    };

    let known = SettingsWindow::open(&window, &second, &gpu_names(), &PawnIoStatus::NotInstalled).unwrap();
    let gone = SettingsWindow::open(&window, &unknown, &gpu_names(), &PawnIoStatus::NotInstalled).unwrap();

    assert_eq!(known.dialog.choice(220), Some(2));
    assert_eq!(gone.dialog.choice(220), Some(0));
}

#[test]
fn a_typed_label_previews() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);

    opened.dialog.set_text(211, "Frames");
    let outcome = opened.on_event(211, DialogChange::Edited);

    let WindowOutcome::Preview(settings) = outcome else {
        panic!("expected a preview, got {outcome:?}");
    };
    assert_eq!(line_of(&settings, Line::Fps).label, "Frames");
}

#[test]
fn ticking_fan_previews_it() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);

    opened.dialog.set_checked(253, true);
    let outcome = opened.on_event(253, DialogChange::Clicked);

    let WindowOutcome::Preview(settings) = outcome else {
        panic!("expected a preview, got {outcome:?}");
    };
    assert_eq!(
        line_of(&settings, Line::Gpu).fields,
        [Field::Load, Field::Temperature, Field::Fan]
    );
}

#[test]
fn choosing_an_adapter_previews_its_name() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);
    let mut choices = vec!["Automatic (most video memory)".to_owned()];
    choices.extend(gpu_names());

    opened.dialog.set_choices(220, &choices, 1);
    let outcome = opened.on_event(220, DialogChange::Selected);

    let WindowOutcome::Preview(settings) = outcome else {
        panic!("expected a preview, got {outcome:?}");
    };
    assert_eq!(settings.gpu.as_deref(), Some("Radeon RX 9060 XT"));
}

#[test]
fn unticking_a_line_disables_its_value_boxes() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);

    opened.dialog.set_checked(201, false);
    let outcome = opened.on_event(201, DialogChange::Clicked);

    let WindowOutcome::Preview(settings) = outcome else {
        panic!("expected a preview, got {outcome:?}");
    };
    assert!(!line_of(&settings, Line::Fps).enabled);
    for value in [231, 232, 233] {
        assert!(!opened.dialog.is_enabled(value), "FPS value {value} must be disabled");
    }
    assert!(opened.dialog.is_enabled(241), "the CPU line's values stay enabled");
}

#[test]
fn a_bad_interval_shows_its_problem_and_does_not_preview() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);

    opened.dialog.set_text(221, "abc");
    let outcome = opened.on_event(221, DialogChange::Edited);

    assert_eq!(outcome, WindowOutcome::Nothing);
    assert_eq!(opened.dialog.text(400), "Update every: enter seconds from 0.25 to 5.");
}

#[test]
fn fixing_the_field_clears_the_problem_shown() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);
    opened.dialog.set_text(221, "abc");
    opened.on_event(221, DialogChange::Edited);

    opened.dialog.set_text(221, "1");
    opened.on_event(221, DialogChange::Edited);

    assert_eq!(opened.dialog.text(400), "");
}

#[test]
fn ok_with_a_problem_left_shows_it_and_does_not_finish() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);
    opened.dialog.set_text(106, "9");
    opened.on_event(106, DialogChange::Edited);
    opened.dialog.set_text(400, "");

    let outcome = opened.on_event(1, DialogChange::Clicked);

    assert_eq!(outcome, WindowOutcome::Nothing);
    assert_eq!(
        opened.dialog.text(400),
        "Outline width: enter whole pixels from 0 to 8."
    );
}

#[test]
fn cancel_gives_back_the_settings_the_window_opened_with() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);
    opened.dialog.set_text(221, "3");
    opened.on_event(221, DialogChange::Edited);

    let outcome = opened.on_event(2, DialogChange::Clicked);

    assert_eq!(outcome, WindowOutcome::Cancelled(Settings::default()));
}

#[test]
fn the_live_view_state_wins_over_the_edited_copy() {
    let live = Settings {
        visible: false,
        locked: true,
        position: Some(OverlayPosition {
            monitor: r"\\.\DISPLAY2".to_owned(),
            x: 10,
            y: 20,
        }),
        ..Settings::default()
    };
    let edited = Settings {
        update_interval: Duration::from_secs(2),
        ..Settings::default()
    };

    let kept = keep_live_view_state(edited, &live);

    assert_eq!(
        kept,
        Settings {
            update_interval: Duration::from_secs(2),
            ..live
        }
    );
}

#[test]
#[ignore = "visual check: keeps the settings window open for a screenshot"]
fn the_settings_window_stays_open_for_a_look() {
    let window = MessageWindow::create().unwrap();
    let _opened = open(&window);

    pump_messages_for(Duration::from_secs(8));
}

fn control(template: &platform_win32::dialog::DialogTemplate, id: u16) -> Vec<&platform_win32::dialog::DialogItem> {
    template.items.iter().filter(|item| item.id == id).collect()
}

#[test]
fn the_general_controls_are_in_the_template_once() {
    let template = settings_template();

    let kinds: Vec<_> = (301..=305)
        .map(|id| control(&template, id).iter().map(|item| item.kind).collect::<Vec<_>>())
        .collect();

    assert_eq!(
        kinds,
        [
            vec![ItemKind::HotKey],
            vec![ItemKind::HotKey],
            vec![ItemKind::CheckBox],
            vec![ItemKind::Label],
            vec![ItemKind::Button]
        ]
    );
    let texts: Vec<&str> = [303, 305].map(|id| control(&template, id)[0].text.as_str()).to_vec();
    assert_eq!(texts, ["Start with Windows", "Install\u{2026}"]);
    let captions: Vec<&str> = template
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Label)
        .map(|item| item.text.as_str())
        .collect();
    assert!(captions.contains(&"Show/hide") && captions.contains(&"Lock/unlock"));
}

#[test]
fn opening_shows_the_drivers_status() {
    let window = MessageWindow::create().unwrap();

    let opened =
        SettingsWindow::open(&window, &Settings::default(), &gpu_names(), &PawnIoStatus::NotInstalled).unwrap();

    assert_eq!(opened.dialog.text(304), "Sensor driver (PawnIO): not installed");
    assert!(opened.dialog.is_enabled(305));
}

#[test]
fn opening_with_an_installed_driver_leaves_install_off() {
    let window = MessageWindow::create().unwrap();
    let installed = PawnIoStatus::Installed {
        version: "2.1.0".to_owned(),
    };

    let opened = SettingsWindow::open(&window, &Settings::default(), &gpu_names(), &installed).unwrap();

    assert_eq!(
        opened.dialog.text(304),
        "Sensor driver (PawnIO): installed, version 2.1.0 \u{2713}"
    );
    assert!(!opened.dialog.is_enabled(305));
}

#[test]
fn the_install_button_is_offered_only_when_pawnio_is_missing_or_too_old() {
    let offered = |status: PawnIoStatus| offers_install(&status);

    assert!(offered(PawnIoStatus::NotInstalled));
    assert!(offered(PawnIoStatus::TooOld { version: None }));
    assert!(offered(PawnIoStatus::Unanswered {
        reason: "no answer".to_owned()
    }));
    assert!(!offered(PawnIoStatus::Installed {
        version: "2.1.0".to_owned()
    }));
    assert!(!offered(PawnIoStatus::NeedsAdmin));
    assert_eq!(PAWNIO_URL, "https://pawnio.eu/");
}

#[test]
fn clicking_install_asks_the_ui_to_open_the_page() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);

    assert_eq!(
        opened.on_event(305, DialogChange::Clicked),
        WindowOutcome::InstallPawnio
    );
}

#[test]
fn opening_fills_the_shortcut_fields_and_the_start_box() {
    let window = MessageWindow::create().unwrap();
    let settings = Settings {
        start_with_windows: true,
        ..Settings::default()
    };

    let opened = SettingsWindow::open(&window, &settings, &gpu_names(), &PawnIoStatus::NotInstalled).unwrap();

    assert_eq!(opened.dialog.hotkey(301), hotkey_keys(settings.show_hide));
    assert_eq!(opened.dialog.hotkey(302), hotkey_keys(settings.lock_unlock));
    assert!(opened.dialog.is_checked(303));
}

#[test]
fn a_new_shortcut_in_a_field_previews_and_a_clash_is_refused() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);
    let keys = |key| {
        hotkey_keys(ShortcutKeys {
            ctrl: true,
            shift: false,
            alt: true,
            key,
        })
    };

    opened.dialog.set_hotkey(301, keys(b'K'));
    let changed = opened.on_event(301, DialogChange::Edited);
    opened.dialog.set_hotkey(302, keys(b'K'));
    let clash = opened.on_event(302, DialogChange::Edited);

    let WindowOutcome::Preview(settings) = changed else {
        panic!("expected a preview, got {changed:?}");
    };
    assert_eq!(settings.show_hide.key, b'K');
    assert_eq!(clash, WindowOutcome::Nothing);
    assert_eq!(
        opened.dialog.text(400),
        "Show/hide and lock/unlock need different shortcuts."
    );
}

#[test]
fn a_ticked_start_box_reaches_the_finished_settings() {
    let window = MessageWindow::create().unwrap();
    let mut opened = open(&window);

    opened.dialog.set_checked(303, true);
    opened.on_event(303, DialogChange::Clicked);
    let outcome = opened.on_event(1, DialogChange::Clicked);

    let WindowOutcome::Finished(settings) = outcome else {
        panic!("expected the end of the edit, got {outcome:?}");
    };
    assert!(settings.start_with_windows);
}
