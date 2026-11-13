//! The time port. The domain takes time as a value; a use case reads it from a [`Clock`]; only [`SystemClock`]
//! reads the real one, so a test can fix the time with [`FixedClock`].

pub use jiff::Timestamp;

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        #[expect(
            clippy::disallowed_methods,
            reason = "this is the clock adapter every other read goes through"
        )]
        Timestamp::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}
