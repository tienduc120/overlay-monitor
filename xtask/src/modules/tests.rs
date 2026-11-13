use super::{check_hash, parse_certutil_sha256};

#[test]
fn the_hash_is_the_second_line_of_certutil_output_without_spaces() {
    let out = "SHA256 hash of x.bin:\r\ndae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91\r\nCertUtil: -hashfile command completed successfully.\r\n";
    assert_eq!(
        parse_certutil_sha256(out).as_deref(),
        Some("dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91")
    );
}

#[test]
fn certutil_output_without_a_hash_line_gives_nothing() {
    assert_eq!(parse_certutil_sha256("CertUtil: -hashfile command FAILED: 0x2"), None);
    assert_eq!(parse_certutil_sha256("header\r\nnot-a-hash\r\n"), None);
}

#[test]
fn a_matching_hash_passes_in_either_case() {
    check_hash("a.bin", "ABCDEF", "abcdef").unwrap();
}

#[test]
fn a_mismatch_names_the_file_and_both_hashes() {
    let message = format!("{:#}", check_hash("AMDFamily17.bin", "aaaa", "bbbb").unwrap_err());

    assert!(message.contains("AMDFamily17.bin"), "{message}");
    assert!(message.contains("aaaa") && message.contains("bbbb"), "{message}");
}
