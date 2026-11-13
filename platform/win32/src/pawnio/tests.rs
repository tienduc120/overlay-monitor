use super::{PawnError, PawnVersion, decode_version, driver_version, encode_execute};

#[test]
fn execute_input_is_the_name_padded_to_32_bytes_then_the_arguments() {
    let buf = encode_execute("ioctl_read_smn", &[0x0005_9800]).unwrap();
    assert_eq!(buf.len(), 40);
    assert_eq!(&buf[..14], b"ioctl_read_smn");
    assert!(buf[14..32].iter().all(|&b| b == 0));
    assert_eq!(&buf[32..], &0x0005_9800u64.to_le_bytes());
}

#[test]
fn a_name_of_32_bytes_or_more_is_refused() {
    assert!(matches!(
        encode_execute(&"x".repeat(32), &[]),
        Err(PawnError::NameTooLong)
    ));
}

#[test]
fn the_version_word_is_major_minor_patch_bytes() {
    let v = |major, minor, patch| PawnVersion { major, minor, patch };

    assert_eq!(decode_version(0x0002_0200), v(2, 2, 0));
    assert_eq!(decode_version(0x0002_0100), v(2, 1, 0));
    assert_eq!(decode_version(0x0002_0100).to_string(), "2.1.0");
    assert!(decode_version(0x0002_0009) < decode_version(0x0002_0100));
}

#[test]
#[ignore = "needs PawnIO and an elevated terminal"]
fn the_installed_driver_reports_2_1_0_or_later() {
    let version = driver_version().unwrap();

    assert!(version >= decode_version(0x0002_0100), "{version}");
}
