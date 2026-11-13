//! Fixture.
pub async fn bump(lock: &tokio::sync::Mutex<u8>) {
    let mut guard = lock.lock().await;
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    *guard += 1;
}
