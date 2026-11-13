//! Fixture.
pub const fn zero() -> u8 {
    unsafe { core::mem::transmute::<i8, u8>(0) }
}
