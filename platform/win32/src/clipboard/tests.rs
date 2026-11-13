use super::*;

#[test]
#[ignore = "writes the system clipboard"]
fn text_put_on_the_clipboard_reads_back() {
    let window = MessageWindow::create().expect("a message window");
    let report = "Overlay Monitor 0.1.0\r\nPawnIO: installed ✓\r\n";

    set_text(&window, report).expect("the clipboard takes text");

    assert_eq!(text().as_deref(), Some(report));
}
