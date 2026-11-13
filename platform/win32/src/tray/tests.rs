use crate::tray::TrayIcon;
use crate::ui::MessageWindow;

#[test]
fn the_icon_can_be_added_added_again_and_removed() {
    let window = MessageWindow::create().unwrap();

    let icon = TrayIcon::add(&window, "Overlay Monitor test").unwrap();
    let again = icon.readd();
    drop(icon);

    assert!(
        again.is_ok(),
        "re-adding after an Explorer restart must work: {again:?}"
    );
}
