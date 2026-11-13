//! Fixture.

#[expect(unsafe_code, reason = "fixture: unsafe outside platform-win32")]
pub fn first(bytes: &[u8; 1]) -> u8 {
    // SAFETY: the array has one element, so the pointer is valid for one read.
    unsafe { *bytes.as_ptr() }
}
