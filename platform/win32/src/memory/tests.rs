use super::memory_status;

#[test]
fn available_memory_never_exceeds_total() {
    let m = memory_status().unwrap();
    assert!(m.total_phys > 0 && m.avail_phys <= m.total_phys, "{m:?}");
}
