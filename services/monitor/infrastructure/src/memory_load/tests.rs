use platform_win32::memory::MemoryStatus;

use super::load_percent;

#[test]
fn load_is_used_over_total_memory() {
    let gib = 1u64 << 30;
    let load = load_percent(MemoryStatus {
        total_phys: 32 * gib,
        avail_phys: 16 * gib,
    })
    .unwrap();
    assert!((load - 50.0).abs() < 1e-9);
}

#[test]
fn an_empty_total_gives_no_load() {
    assert_eq!(
        load_percent(MemoryStatus {
            total_phys: 0,
            avail_phys: 0
        }),
        None
    );
}
