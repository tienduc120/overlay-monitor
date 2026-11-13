use super::{temperature_fact, windows_name};

#[test]
fn windows_11_is_named_from_its_build_number() {
    assert_eq!(
        windows_name("Windows 10 Pro", "25H2", 26300, 7171),
        "Windows 11 Pro 25H2 (build 26300.7171)"
    );
}

#[test]
fn windows_10_keeps_its_name() {
    assert_eq!(
        windows_name("Windows 10 Home", "22H2", 19045, 5011),
        "Windows 10 Home 22H2 (build 19045.5011)"
    );
}

#[test]
fn a_missing_display_version_leaves_no_gap() {
    assert_eq!(
        windows_name("Windows 10 Pro", "", 19041, 1),
        "Windows 10 Pro (build 19041.1)"
    );
}

#[test]
fn a_temperature_of_zero_means_the_driver_reports_none() {
    assert_eq!(
        temperature_fact(0),
        Err(String::from("the driver reports no temperature"))
    );
    assert_eq!(temperature_fact(563), Ok(56.3));
}
