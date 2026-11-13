use monitor_application::Sensor;
use platform_win32::gpu::{GpuAdapter, gpu_adapters};

use super::{GpuTemperature, adapter_for, engine_load_percent, pick_adapter};

fn adapter(name: &str, luid: u64, gib: u64) -> GpuAdapter {
    GpuAdapter {
        luid,
        name: name.to_owned(),
        dedicated_bytes: gib << 30,
    }
}

#[test]
fn the_adapter_with_the_most_dedicated_memory_is_followed() {
    let list = [
        adapter("AMD Radeon(TM) Graphics", 1, 0),
        adapter("AMD Radeon RX 9060 XT", 0xD3E5, 16),
    ];
    assert_eq!(pick_adapter(&list).unwrap().name, "AMD Radeon RX 9060 XT");
}

#[test]
fn a_named_adapter_is_followed_and_an_unknown_name_falls_back_to_the_most_memory() {
    let list = [adapter("Small", 1, 0), adapter("Big", 2, 16)];
    assert_eq!(adapter_for(&list, Some("Small")).unwrap().luid, 1);
    assert_eq!(adapter_for(&list, Some("Gone")).unwrap().luid, 2);
    assert_eq!(adapter_for(&list, None).unwrap().luid, 2);
    assert!(adapter_for(&[], Some("Small")).is_none());
}

#[test]
fn load_is_the_busiest_engine_type_summed_over_processes_for_that_adapter() {
    let s = |n: &str, v: f64| (n.to_owned(), v);
    let samples = [
        s("pid_1234_luid_0x00000000_0x0000D3E5_phys_0_eng_0_engtype_3D", 40.0),
        s("pid_99_luid_0x00000000_0x0000d3e5_phys_0_eng_0_engtype_3D", 15.0),
        s(
            "pid_99_luid_0x00000000_0x0000D3E5_phys_0_eng_5_engtype_VideoDecode",
            20.0,
        ),
        s("pid_7_luid_0x00000000_0x00000001_phys_0_eng_0_engtype_3D", 90.0),
    ];
    assert_eq!(
        engine_load_percent(&samples, 0xD3E5),
        Some(55.0),
        "3D 40+15 beats decode 20; other adapter ignored"
    );
}

#[test]
fn no_engine_for_that_adapter_gives_no_load() {
    assert_eq!(engine_load_percent(&[], 0xD3E5), None);
}

#[test]
fn load_never_reads_above_100_percent() {
    let s = [(
        "pid_1_luid_0x00000000_0x0000D3E5_phys_0_eng_0_engtype_3D".to_owned(),
        130.0,
    )];
    assert_eq!(engine_load_percent(&s, 0xD3E5), Some(100.0));
}

#[test]
#[ignore = "needs a GPU whose driver reports temperature"]
fn the_followed_gpu_reports_a_plausible_temperature() {
    let a = pick_adapter(&gpu_adapters().unwrap()).cloned().unwrap();
    let t = GpuTemperature::new(a.luid).read().unwrap();
    assert!((20.0..=100.0).contains(&t), "{} {t}", a.name);
}
