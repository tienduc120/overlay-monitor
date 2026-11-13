use super::{is_module_loaded, memory_status, process_memory, trim_working_set};

#[test]
fn available_memory_never_exceeds_total() {
    let m = memory_status().unwrap();
    assert!(m.total_phys > 0 && m.avail_phys <= m.total_phys, "{m:?}");
}

#[test]
fn this_process_holds_memory_and_its_peak_is_not_below_it() {
    let m = process_memory().unwrap();

    assert!(m.private_bytes > 0 && m.working_set > 0, "{m:?}");
    assert!(m.peak_working_set >= m.working_set, "{m:?}");
}

#[test]
fn a_loaded_module_is_found_and_an_absent_one_is_not() {
    assert!(is_module_loaded("kernel32.dll"));
    assert!(!is_module_loaded("overlay-monitor-no-such-module.dll"));
}

#[test]
fn trimming_the_working_set_succeeds() {
    trim_working_set().unwrap();
}
