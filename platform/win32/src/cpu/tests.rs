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

use super::{CpuVendor, brand_from_registers, cpu_identity, decode_signature};

#[test]
fn the_7800x3d_signature_is_family_19h_model_61h() {
    assert_eq!(decode_signature(0x00A6_0F12), (0x19, 0x61, 2));
}

#[test]
fn an_alder_lake_signature_is_family_6_model_97h() {
    assert_eq!(decode_signature(0x0009_0672), (6, 0x97, 2));
}

#[test]
fn the_brand_is_trimmed_of_padding() {
    let text = b"  AMD Ryzen 7 7800X3D 8-Core Processor\0\0\0\0\0\0\0\0\0\0";
    let mut padded = [0u8; 48];
    padded[..text.len().min(48)].copy_from_slice(&text[..text.len().min(48)]);
    let mut words = [0u32; 12];
    for (word, chunk) in words.iter_mut().zip(padded.as_chunks::<4>().0) {
        *word = u32::from_le_bytes(*chunk);
    }

    assert_eq!(brand_from_registers(&words), "AMD Ryzen 7 7800X3D 8-Core Processor");
}

#[test]
fn this_cpu_reports_a_vendor_and_a_brand() {
    let cpu = cpu_identity();

    assert!(!cpu.vendor_id.is_empty() && !cpu.brand.is_empty(), "{cpu:?}");
    assert_ne!(cpu.family, 0, "{cpu:?}");
    let expected = match cpu.vendor_id.as_str() {
        "AuthenticAMD" => CpuVendor::Amd,
        "GenuineIntel" => CpuVendor::Intel,
        _ => CpuVendor::Other,
    };
    assert_eq!(cpu.vendor, expected);
}
