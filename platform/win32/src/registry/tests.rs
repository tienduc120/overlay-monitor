use super::*;

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[test]
fn the_current_build_number_reads_as_text() {
    let build = read_string(CURRENT_VERSION, "CurrentBuildNumber").expect("every Windows has a build number");
    assert!(build.parse::<u32>().is_ok(), "{build:?}");
}

#[test]
fn a_number_value_reads_as_a_number() {
    let major = read_u32(CURRENT_VERSION, "CurrentMajorVersionNumber").expect("Windows 10 and 11 have one");
    assert_eq!(major, 10);
}

#[test]
fn a_missing_value_is_an_error() {
    read_string(CURRENT_VERSION, "NoSuchValueHere").unwrap_err();
}
