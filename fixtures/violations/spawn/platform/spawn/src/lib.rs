//! Fixture.
pub fn detach() {
    drop(std::thread::spawn(|| {}));
}
