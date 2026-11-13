use platform_win32::ui::UiEvent;

use super::{Action, UiState, menu_items};

#[test]
fn a_tray_click_toggles_the_overlay() {
    let mut s = UiState { visible: true };
    assert_eq!(s.on_event(UiEvent::TrayClick), vec![Action::HideOverlay]);
    assert_eq!(
        s.on_event(UiEvent::TrayClick),
        vec![Action::ShowOverlay, Action::DrawLatest]
    );
}

#[test]
fn both_menus_open_at_the_pointer() {
    let mut s = UiState { visible: true };
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
    let items = menu_items(true);
    assert_eq!(
        items
            .iter()
            .map(|i| (i.id, i.text.as_str(), i.checked))
            .collect::<Vec<_>>(),
        vec![(1, "Show overlay", true), (2, "Exit", false)]
    );
}

#[test]
fn menu_commands_toggle_and_exit() {
    let mut s = UiState { visible: false };
    assert_eq!(s.on_command(1), vec![Action::ShowOverlay, Action::DrawLatest]);
    assert_eq!(s.on_command(2), vec![Action::Exit]);
}

#[test]
fn a_wake_draws_only_while_visible() {
    assert_eq!(
        UiState { visible: true }.on_event(UiEvent::Wake),
        vec![Action::DrawLatest]
    );
    assert_eq!(UiState { visible: false }.on_event(UiEvent::Wake), Vec::<Action>::new());
}

#[test]
fn the_tray_icon_comes_back_after_explorer_restarts() {
    assert_eq!(
        UiState { visible: true }.on_event(UiEvent::TaskbarRestarted),
        vec![Action::ReaddTrayIcon]
    );
}

#[test]
fn an_unknown_menu_command_does_nothing() {
    assert_eq!(UiState { visible: true }.on_command(99), Vec::<Action>::new());
}

#[test]
fn the_menu_command_for_the_overlay_hides_it_while_visible() {
    let mut s = UiState { visible: true };
    assert_eq!(s.on_command(1), vec![Action::HideOverlay]);
    assert!(!s.visible, "the state must follow the overlay");
}
