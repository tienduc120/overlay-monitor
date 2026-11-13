use super::resource_script;

const ICON: &str = "C:/work/assets/icon.ico";

#[test]
fn the_script_names_the_app_and_carries_the_version_in_both_forms() {
    let script = resource_script("0.1.0", ICON);

    assert!(script.contains("FILEVERSION 0,1,0,0"), "{script}");
    assert!(script.contains("PRODUCTVERSION 0,1,0,0"), "{script}");
    assert!(script.contains("VALUE \"FileVersion\", \"0.1.0\""), "{script}");
    assert!(script.contains("VALUE \"ProductVersion\", \"0.1.0\""), "{script}");
    assert!(
        script.contains("VALUE \"FileDescription\", \"Overlay Monitor\""),
        "{script}"
    );
    assert!(
        script.contains("VALUE \"ProductName\", \"Overlay Monitor\""),
        "{script}"
    );
    assert!(
        script.contains("VALUE \"InternalName\", \"overlay-monitor.exe\""),
        "{script}"
    );
    assert!(
        script.contains("VALUE \"OriginalFilename\", \"overlay-monitor.exe\""),
        "{script}"
    );
    assert!(
        script.contains("VALUE \"LegalCopyright\", \"Copyright (c) 2026 Luu Tien Duc\""),
        "{script}"
    );
}

#[test]
fn a_pre_release_keeps_its_suffix_in_the_text_and_drops_it_from_the_numbers() {
    let script = resource_script("1.22.333-rc.1", ICON);

    assert!(script.contains("FILEVERSION 1,22,333,0"), "{script}");
    assert!(script.contains("VALUE \"FileVersion\", \"1.22.333-rc.1\""), "{script}");
}

#[test]
fn the_icon_is_resource_one_at_the_path_given() {
    assert!(resource_script("0.1.0", ICON).contains("1 ICON \"C:/work/assets/icon.ico\""));
}

#[test]
fn a_version_that_is_not_numbers_gives_zeros_rather_than_a_broken_script() {
    assert!(resource_script("dev", ICON).contains("FILEVERSION 0,0,0,0"));
}
