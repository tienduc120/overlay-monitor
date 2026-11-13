//! Fixture.
pub fn detach() {
    drop(tokio::spawn(async {}));
}
