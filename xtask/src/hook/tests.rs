use std::path::Path;

use super::relative;

#[test]
fn a_path_inside_the_repository_becomes_relative_whatever_the_drive_letter_case_or_slashes() {
    let root = Path::new(r"D:\Duc\3_Project\rust-base");

    assert_eq!(
        relative(root, Path::new(r"d:\Duc\3_Project\rust-base\clippy.toml")).as_deref(),
        Some("clippy.toml")
    );
    assert_eq!(
        relative(root, Path::new("D:/Duc/3_Project/rust-base/xtask/src/gates/arch.rs")).as_deref(),
        Some("xtask/src/gates/arch.rs")
    );
}

#[test]
fn a_path_outside_the_repository_or_a_sibling_with_the_same_prefix_is_not_ours() {
    let root = Path::new(r"D:\Duc\3_Project\rust-base");

    assert_eq!(
        relative(root, Path::new(r"D:\Duc\3_Project\rust-base-old\clippy.toml")),
        None
    );
    assert_eq!(relative(root, Path::new(r"C:\elsewhere\clippy.toml")), None);
}

#[test]
fn dot_dot_is_resolved_before_the_path_is_judged() {
    let root = Path::new(r"D:\Duc\3_Project\rust-base");

    assert_eq!(
        relative(root, Path::new(r"D:\Duc\3_Project\rust-base\xtask\..\Cargo.toml")).as_deref(),
        Some("Cargo.toml")
    );
    assert_eq!(
        relative(root, Path::new(r"D:\Duc\3_Project\rust-base")),
        None,
        "the root is not a file"
    );
}

#[test]
fn committed_is_decided_against_heads_file_list_whatever_the_case() {
    let head = "Cargo.toml\nxtask/src/gates/arch.rs\n";

    assert!(super::listed(head, "cargo.TOML"));
    assert!(super::listed(head, "XTASK/src/gates/arch.rs"));
    assert!(!super::listed(head, "clippy.toml"));
}
