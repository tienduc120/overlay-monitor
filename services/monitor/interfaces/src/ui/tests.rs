use std::sync::mpsc::{TryRecvError, channel};

use monitor_application::testing::MemoryStore;
use monitor_application::{OverlayFrame, SamplerControl, Settings, ShortcutKeys};
use platform_win32::hotkey::Hotkey;
use platform_win32::icon::Icon;
use platform_win32::overlay::OverlayWindow;
use platform_win32::tray::TrayIcon;
use platform_win32::ui::{Flow, MessageWindow, UiEvent};

use crate::{OverlayView, hotkey_keys};

use super::{Action, ErrorLog, LOCK_HOTKEY, SHOW_HIDE_HOTKEY, Ui, UiParts, UiState, menu_items, restore_focus_after};

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
fn the_menu_shows_a_check_while_visible_and_offers_exit() {
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
    let tray = TrayIcon::add(&window, "Overlay Monitor test", &image).unwrap();
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
