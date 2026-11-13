use super::cpu_times;

#[test]
fn kernel_time_includes_idle_time() {
    let t = cpu_times().unwrap();
    assert!(
        t.kernel >= t.idle,
        "GetSystemTimes counts idle time inside kernel time: {t:?}"
    );
}

#[test]
fn every_counter_only_grows() {
    let a = cpu_times().unwrap();
    let b = cpu_times().unwrap();
    assert!(
        b.idle >= a.idle && b.kernel >= a.kernel && b.user >= a.user,
        "{a:?} then {b:?}"
    );
}
