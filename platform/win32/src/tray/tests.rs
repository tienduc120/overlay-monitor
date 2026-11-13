use crate::icon::Icon;
use crate::tray::TrayIcon;
use crate::ui::MessageWindow;

#[test]
fn the_icon_can_be_added_added_again_and_removed() {
    let window = MessageWindow::create().unwrap();

    let image = Icon::stock().unwrap();
    let icon = TrayIcon::create(&window, "Overlay Monitor test", &image);
    icon.readd().unwrap();
    let again = icon.readd();
    drop(icon);

    assert!(
        again.is_ok(),
        "re-adding after an Explorer restart must work: {again:?}"
    );
}

#[test]
fn a_tray_icon_made_before_the_shell_is_ready_is_added_later() {
    let window = MessageWindow::create().unwrap();
    let image = Icon::stock().unwrap();

    let icon = TrayIcon::create(&window, "Overlay Monitor test", &image);
    let added = icon.readd();

    assert!(added.is_ok(), "the first add may come through readd: {added:?}");
}
