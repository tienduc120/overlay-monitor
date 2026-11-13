use monitor_application::Sensor;
use platform_win32::gpu::{GpuAdapter, gpu_adapters};

use super::{
    GpuTemperature, adapter_for, engine_load_percent, fan_value, memory_mhz, pick_adapter, video_load_percent,
};

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
        adapter("AMD Radeon RX 7800 XT", 0xD3E5, 16),
    ];
    assert_eq!(pick_adapter(&list).unwrap().name, "AMD Radeon RX 7800 XT");
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
    let t = GpuTemperature::new(a.luid, &a.name).read().unwrap();
    assert!((20.0..=100.0).contains(&t), "{} {t}", a.name);
}

#[test]
fn a_fan_at_0_rpm_shows_only_after_it_has_spun() {
    let mut seen_spinning = false;
    assert_eq!(
        fan_value(0, &mut seen_spinning),
        None,
        "a driver that never reports leaves 0"
    );
    assert_eq!(fan_value(1200, &mut seen_spinning), Some(1200.0));
    assert_eq!(
        fan_value(0, &mut seen_spinning),
        Some(0.0),
        "a fan that stopped is real"
    );
}

#[test]
fn a_vram_clock_of_0_is_not_reported() {
    assert_eq!(memory_mhz(0), None);
    assert_eq!(memory_mhz(644_000_000), Some(644.0));
}

fn engines(list: &[(&str, f64)]) -> Vec<(String, f64)> {
    list.iter()
        .map(|(engine, value)| {
            (
                format!("pid_1_luid_0x00000000_0x0000D3E5_phys_0_eng_0_engtype_{engine}"),
                *value,
            )
        })
        .collect()
}

#[test]
fn video_load_is_the_busiest_video_engine_without_jpeg() {
    let amd = engines(&[
        ("3D", 80.0),
        ("Video Codec 0", 10.0),
        ("Video Decode 1", 25.0),
        ("Video JPEG 0", 90.0),
    ]);
    assert_eq!(video_load_percent(&amd, 0xD3E5), Some(25.0));
    let nvidia = engines(&[("3D", 80.0), ("VideoEncode", 40.0)]);
    assert_eq!(video_load_percent(&nvidia, 0xD3E5), Some(40.0));
}

#[test]
fn an_adapter_without_video_engines_has_no_video_load() {
    assert_eq!(
        video_load_percent(&engines(&[("3D", 80.0), ("Copy", 5.0)]), 0xD3E5),
        None
    );
    assert_eq!(video_load_percent(&[], 0xD3E5), None);
}

#[test]
fn the_gpu_sensors_name_their_adapter_in_the_details() {
    assert_eq!(
        GpuTemperature::new(0xD3E5, "AMD Radeon RX 7800 XT")
            .details()
            .as_deref(),
        Some("adapter AMD Radeon RX 7800 XT")
    );
}
