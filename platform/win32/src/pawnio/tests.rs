use super::{PawnError, encode_execute};

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
