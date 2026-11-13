use std::sync::mpsc::{TryRecvError, channel};
use std::time::{Duration, Instant};

use monitor_application::testing::MemoryStore;
use monitor_application::{OverlayFrame, SamplerControl, Settings, ShortcutKeys, overlay_style};
use platform_win32::dialog::{Dialog, DialogChange};
use platform_win32::hotkey::Hotkey;
use platform_win32::icon::Icon;
use platform_win32::overlay::OverlayWindow;
use platform_win32::tray::TrayIcon;
use platform_win32::ui::{Flow, MessageWindow, UiEvent};

use crate::{OverlayView, hotkey_keys};

use super::{
    Action, ErrorLog, LOCK_HOTKEY, SETTINGS, SHOW_HIDE_HOTKEY, Ui, UiParts, UiState, frames_stalled, menu_items,
    restore_focus_after,
};

#[test]
fn a_tray_click_toggles_and_saves() {
    let mut s = UiState {
        visible: true,
        locked: false,
    };
    assert_eq!(
        s.on_event(UiEvent::TrayClick),
        vec![Action::HideOverlay, Action::SaveSettings]
    );
    assert_eq!(
        s.on_event(UiEvent::TrayClick),
        vec![Action::ShowOverlay, Action::DrawLatest, Action::SaveSettings]
    );
}

#[test]
fn a_finished_drag_asks_to_remember_the_position() {
    let mut s = UiState {
        visible: true,
        locked: false,
    };
    assert_eq!(s.on_event(UiEvent::OverlayMoved), vec![Action::RememberPosition]);
}

#[test]
fn both_menus_open_at_the_pointer() {
    let mut s = UiState {
        visible: true,
        locked: false,
    };
    assert_eq!(
        s.on_event(UiEvent::TrayMenu { x: 5, y: 6 }),
        vec![Action::ShowMenu { x: 5, y: 6 }]
    );
    assert_eq!(
        s.on_event(UiEvent::OverlayMenu { x: 7, y: 8 }),
        vec![Action::ShowMenu { x: 7, y: 8 }]
    );
}

#[test]
fn the_menu_offers_settings() {
    let items = menu_items(&UiState {
        visible: true,
        locked: false,
    });
    assert_eq!(
        items
            .iter()
            .map(|i| (i.id, i.text.as_str(), i.checked))
            .collect::<Vec<_>>(),
        vec![
            (1, "Show overlay", true),
            (3, "Lock position", false),
            (4, "Settings\u{2026}", false),
            (2, "Exit", false)
        ]
    );
}

#[test]
fn menu_commands_toggle_and_exit() {
    let mut s = UiState {
        visible: false,
        locked: false,
    };
    assert_eq!(
        s.on_command(1),
        vec![Action::ShowOverlay, Action::DrawLatest, Action::SaveSettings]
    );
    assert_eq!(s.on_command(2), vec![Action::Exit]);
}

#[test]
fn a_wake_draws_only_while_visible() {
    assert_eq!(
        UiState {
            visible: true,
            locked: false
        }
        .on_event(UiEvent::Wake),
        vec![Action::DrawLatest]
    );
    assert_eq!(
        UiState {
            visible: false,
            locked: false
        }
        .on_event(UiEvent::Wake),
        Vec::<Action>::new()
    );
}

#[test]
fn the_tray_icon_comes_back_after_explorer_restarts() {
    assert_eq!(
        UiState {
            visible: true,
            locked: false
        }
        .on_event(UiEvent::TaskbarRestarted),
        vec![Action::ReaddTrayIcon]
    );
}

#[test]
fn a_dialog_event_without_an_open_dialog_does_nothing() {
    let event = UiEvent::Dialog {
        control: 1,
        change: DialogChange::Clicked,
    };

    assert_eq!(
        UiState {
            visible: true,
            locked: false
        }
        .on_event(event),
        Vec::<Action>::new()
    );
}

#[test]
fn an_unknown_menu_command_does_nothing() {
    assert_eq!(
        UiState {
            visible: true,
            locked: false
        }
        .on_command(99),
        Vec::<Action>::new()
    );
}

#[test]
fn the_menu_command_for_the_overlay_hides_it_while_visible() {
    let mut s = UiState {
        visible: true,
        locked: false,
    };
    assert_eq!(s.on_command(1), vec![Action::HideOverlay, Action::SaveSettings]);
    assert!(!s.visible, "the state must follow the overlay");
}

#[test]
fn a_failure_that_repeats_is_logged_once() {
    let mut log = ErrorLog::default();

    let seen = [Some(5), Some(5), Some(5)].map(|failure| log.is_new(failure));

    assert_eq!(seen, [true, false, false]);
}

#[test]
fn a_different_failure_is_new_and_so_is_one_after_a_success() {
    let mut log = ErrorLog::default();

    let seen = [Some(5), Some(6), None, Some(6)].map(|failure| log.is_new(failure));

    assert_eq!(seen, [true, true, false, true]);
}

/// A UI over hidden windows, a store the test keeps a handle to, and the sampler's end of the control channel.
struct Rig {
    window: MessageWindow,
    ui: Ui,
    store: MemoryStore,
    frames: std::sync::mpsc::Sender<OverlayFrame>,
    controls: std::sync::mpsc::Receiver<SamplerControl>,
}

fn rig(settings: Settings, store: MemoryStore) -> Rig {
    rig_on(MessageWindow::create().unwrap(), settings, store)
}

fn rig_on(window: MessageWindow, settings: Settings, store: MemoryStore) -> Rig {
    let overlay = OverlayWindow::create(window.sink()).unwrap();
    let image = Icon::stock().unwrap();
    let tray = TrayIcon::create(&window, "Overlay Monitor test", &image);
    let (frames, received) = channel();
    let (sampler, controls) = channel();
    let ui = Ui::new(
        &window,
        UiParts {
            view: OverlayView::new(overlay),
            tray,
            frames: received,
            sampler,
            settings,
            store: Box::new(store.clone()),
            gpu_names: vec!["First GPU".to_owned(), "Second GPU".to_owned()],
        },
    );
    Rig {
        window,
        ui,
        store,
        frames,
        controls,
    }
}

#[test]
fn frames_that_arrive_while_the_overlay_is_hidden_are_kept_only_as_the_newest() {
    let Rig {
        window, mut ui, frames, ..
    } = rig(
        Settings {
            visible: false,
            ..Settings::default()
        },
        MemoryStore::empty(),
    );
    for n in 0..5 {
        frames
            .send(OverlayFrame {
                lines: vec![format!("FPS: {n}")],
            })
            .unwrap();
    }

    let flow = ui.handle(&window, UiEvent::Wake);

    assert_eq!(flow, Flow::Continue);
    assert_eq!(
        ui.frames.try_recv(),
        Err(TryRecvError::Empty),
        "a hidden overlay must not let frames pile up"
    );
    assert_eq!(
        ui.latest,
        Some(OverlayFrame {
            lines: vec![String::from("FPS: 4")]
        })
    );
}

#[test]
fn the_overlay_starts_hidden_when_the_settings_say_so() {
    let hidden = rig(
        Settings {
            visible: false,
            ..Settings::default()
        },
        MemoryStore::empty(),
    );
    let shown = rig(Settings::default(), MemoryStore::empty());

    assert!(!hidden.ui.view.is_visible() && !hidden.ui.state.visible);
    assert!(shown.ui.view.is_visible() && shown.ui.state.visible);
}

#[test]
fn a_finished_drag_saves_the_monitor_and_offset() {
    let Rig {
        window, mut ui, store, ..
    } = rig(Settings::default(), MemoryStore::empty());

    ui.handle(&window, UiEvent::OverlayMoved);

    let saved = store.saved();
    let position = saved.last().and_then(|settings| settings.position.as_ref());
    assert!(
        position.is_some_and(|p| !p.monitor.is_empty()),
        "the save must name the monitor: {saved:?}"
    );
}

#[test]
fn a_tray_click_saves_whether_the_overlay_is_visible() {
    let Rig {
        window, mut ui, store, ..
    } = rig(Settings::default(), MemoryStore::empty());

    ui.handle(&window, UiEvent::TrayClick);

    assert_eq!(store.saved().last().map(|settings| settings.visible), Some(false));
}

#[test]
fn a_failed_save_does_not_stop_the_app() {
    let Rig {
        window, mut ui, store, ..
    } = rig(Settings::default(), MemoryStore::failing_saves());

    let flow = ui.handle(&window, UiEvent::OverlayMoved);

    assert_eq!(flow, Flow::Continue);
    assert_eq!(store.saved().len(), 0, "nothing was stored");
}

#[test]
fn the_ui_holds_the_samplers_control_sender_until_it_is_dropped() {
    let Rig { ui, controls, .. } = rig(Settings::default(), MemoryStore::empty());
    assert_eq!(controls.try_recv().unwrap_err(), TryRecvError::Empty);

    drop(ui);

    assert_eq!(controls.try_recv().unwrap_err(), TryRecvError::Disconnected);
}

#[test]
fn the_show_hide_shortcut_toggles_like_a_tray_click() {
    let mut by_shortcut = UiState {
        visible: true,
        locked: false,
    };
    let mut by_click = by_shortcut;

    let shortcut = by_shortcut.on_event(UiEvent::Hotkey { id: SHOW_HIDE_HOTKEY });

    assert_eq!(shortcut, by_click.on_event(UiEvent::TrayClick));
    assert!(!by_shortcut.visible);
}

#[test]
fn the_lock_command_and_shortcut_lock_and_save() {
    let mut by_command = UiState {
        visible: true,
        locked: false,
    };
    let mut by_shortcut = by_command;
    let locking = vec![Action::SetLocked(true), Action::DrawLatest, Action::SaveSettings];

    assert_eq!(by_command.on_command(3), locking);
    assert_eq!(by_shortcut.on_event(UiEvent::Hotkey { id: LOCK_HOTKEY }), locking);
    assert!(by_command.locked && by_shortcut.locked);
    assert_eq!(
        by_command.on_command(3),
        vec![Action::SetLocked(false), Action::DrawLatest, Action::SaveSettings]
    );
}

#[test]
fn an_unknown_hotkey_does_nothing() {
    let mut s = UiState {
        visible: true,
        locked: false,
    };

    assert_eq!(s.on_event(UiEvent::Hotkey { id: 99 }), Vec::<Action>::new());
    assert_eq!(
        s,
        UiState {
            visible: true,
            locked: false
        }
    );
}

#[test]
fn the_menu_shows_both_checks() {
    let items = menu_items(&UiState {
        visible: true,
        locked: true,
    });

    assert_eq!(
        items
            .iter()
            .map(|i| (i.id, i.text.as_str(), i.checked))
            .collect::<Vec<_>>(),
        vec![
            (1, "Show overlay", true),
            (3, "Lock position", true),
            (4, "Settings\u{2026}", false),
            (2, "Exit", false)
        ]
    );
}

#[test]
fn focus_goes_back_after_the_menu_closes() {
    assert!(restore_focus_after(None));
    assert!(restore_focus_after(Some(1)));
}

#[test]
fn opening_settings_keeps_the_focus_on_the_settings_window() {
    let mut s = UiState {
        visible: true,
        locked: false,
    };

    assert!(!restore_focus_after(Some(SETTINGS)));
    assert_eq!(s.on_command(SETTINGS), vec![Action::OpenSettings]);
}

#[test]
fn a_locked_overlay_starts_unlocked_when_its_shortcut_is_taken() {
    // Ctrl+Alt+Shift+F23, which nothing else uses.
    let taken = ShortcutKeys {
        ctrl: true,
        shift: true,
        alt: true,
        key: 0x86,
    };
    let window = MessageWindow::create().unwrap();
    let _holder = Hotkey::register(&window, 9, hotkey_keys(taken)).unwrap();
    let settings = Settings {
        locked: true,
        lock_unlock: taken,
        ..Settings::default()
    };

    let Rig { ui, store, .. } = rig_on(MessageWindow::create().unwrap(), settings, MemoryStore::empty());

    assert!(!ui.state.locked, "the overlay must be grabbable");
    assert_eq!(store.saved().last().map(|settings| settings.locked), Some(false));
}

#[test]
fn three_missed_intervals_count_as_stalled() {
    let last = Instant::now();
    let interval = Duration::from_secs(1);

    assert!(!frames_stalled(last, last + interval * 3, interval));
    assert!(frames_stalled(
        last,
        last + interval * 3 + Duration::from_millis(1),
        interval
    ));
}

#[test]
fn a_tick_checks_only_while_visible() {
    let mut shown = UiState {
        visible: true,
        locked: false,
    };
    let mut hidden = UiState {
        visible: false,
        locked: false,
    };

    assert_eq!(shown.on_event(UiEvent::Tick), vec![Action::CheckStalled]);
    assert_eq!(hidden.on_event(UiEvent::Tick), Vec::<Action>::new());
}

#[test]
fn an_ending_session_exits() {
    let mut s = UiState {
        visible: false,
        locked: false,
    };

    assert_eq!(s.on_event(UiEvent::SessionEnding), vec![Action::Exit]);
}

/// Settings whose shortcuts (Ctrl+Alt+Shift+F21 and F22) neither the owner's running app nor the other tests' keys
/// (F20, F23, F24) use.
fn settings_with_rare_shortcuts(visible: bool) -> Settings {
    let rare = |key| ShortcutKeys {
        ctrl: true,
        shift: true,
        alt: true,
        key,
    };
    Settings {
        visible,
        show_hide: rare(0x84),
        lock_unlock: rare(0x85),
        ..Settings::default()
    }
}

/// Chooses Settings… from the menu.
fn open_settings(rig: &mut Rig) {
    let actions = rig.ui.state.on_command(SETTINGS);
    rig.ui.apply(&rig.window, actions);
}

fn dialog(ui: &Ui) -> &Dialog {
    ui.settings_window
        .as_ref()
        .expect("the settings window is open")
        .dialog()
}

fn user_did(rig: &mut Rig, control: u16, change: DialogChange) {
    rig.ui.handle(&rig.window, UiEvent::Dialog { control, change });
}

/// Every set of settings the sampler was told to follow since the last call.
fn applied(rig: &Rig) -> Vec<Settings> {
    rig.controls
        .try_iter()
        .map(|SamplerControl::Apply(settings)| settings)
        .collect()
}

#[test]
fn cancel_after_previews_restores_the_overlay_and_the_sampler() {
    let original = settings_with_rare_shortcuts(false);
    let mut rig = rig(original.clone(), MemoryStore::empty());
    let saves_before = rig.store.saved().len();
    open_settings(&mut rig);
    let choices = ["Automatic (most video memory)", "First GPU", "Second GPU"].map(str::to_owned);
    dialog(&rig.ui).set_choices(220, &choices, 1);
    user_did(&mut rig, 220, DialogChange::Selected);
    dialog(&rig.ui).set_text(221, "2");
    user_did(&mut rig, 221, DialogChange::Edited);
    dialog(&rig.ui).set_checked(201, false);
    user_did(&mut rig, 201, DialogChange::Clicked);
    let previewed = applied(&rig);
    let last_preview = previewed.last().expect("the previews reached the sampler");
    assert_eq!(
        (
            last_preview.gpu.as_deref(),
            last_preview.update_interval,
            last_preview.lines[0].enabled
        ),
        (Some("First GPU"), Duration::from_secs(2), false)
    );

    user_did(&mut rig, 2, DialogChange::Clicked);

    assert_eq!(
        applied(&rig),
        std::slice::from_ref(&original),
        "the sampler follows the original again"
    );
    assert_eq!(rig.ui.settings, original);
    assert_eq!(rig.ui.style, overlay_style(&original), "the overlay is drawn as before");
    assert_eq!(rig.store.saved().len(), saves_before, "a cancel saves nothing");
    assert!(rig.ui.settings_window.is_none(), "the window closed");
}

#[test]
fn ok_saves_and_closes() {
    let mut rig = rig(settings_with_rare_shortcuts(false), MemoryStore::empty());
    open_settings(&mut rig);
    dialog(&rig.ui).set_text(211, "Frames");
    user_did(&mut rig, 211, DialogChange::Edited);

    user_did(&mut rig, 1, DialogChange::Clicked);

    assert!(rig.ui.settings_window.is_none(), "the window closed");
    assert_eq!(rig.ui.settings.lines[0].label, "Frames");
    assert_eq!(
        rig.store.saved().last().map(|saved| saved.lines[0].label.clone()),
        Some("Frames".to_owned())
    );
}

#[test]
fn ok_keeps_a_visibility_change_made_while_the_window_was_open() {
    let mut rig = rig(settings_with_rare_shortcuts(true), MemoryStore::empty());
    open_settings(&mut rig);
    rig.ui.handle(&rig.window, UiEvent::TrayClick);
    let saves_before = rig.store.saved().len();

    user_did(&mut rig, 1, DialogChange::Clicked);

    let saved = rig.store.saved();
    assert_eq!(saved.len(), saves_before + 1, "OK saves");
    assert_eq!(saved.last().map(|settings| settings.visible), Some(false));
}

#[test]
fn a_save_while_the_window_is_open_keeps_the_previews_out() {
    let original = settings_with_rare_shortcuts(true);
    let mut rig = rig(original.clone(), MemoryStore::empty());
    open_settings(&mut rig);
    dialog(&rig.ui).set_text(221, "2");
    user_did(&mut rig, 221, DialogChange::Edited);

    rig.ui.handle(&rig.window, UiEvent::TrayClick);

    assert_eq!(
        rig.store
            .saved()
            .last()
            .map(|saved| (saved.update_interval, saved.visible)),
        Some((original.update_interval, false)),
        "only the visibility is saved before OK"
    );
}

#[test]
fn opening_settings_again_keeps_the_open_window() {
    let mut rig = rig(settings_with_rare_shortcuts(false), MemoryStore::empty());
    open_settings(&mut rig);
    dialog(&rig.ui).set_text(221, "2");
    user_did(&mut rig, 221, DialogChange::Edited);

    open_settings(&mut rig);

    assert_eq!(dialog(&rig.ui).text(221), "2", "a new window would show the saved 1.0");
}
