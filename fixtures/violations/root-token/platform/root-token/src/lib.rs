//! Fixture.
pub fn token() -> tokio_util::sync::CancellationToken {
    tokio_util::sync::CancellationToken::new()
}
