use super::*;

#[test]
fn the_roaming_and_local_folders_are_existing_absolute_directories() {
    for folder in [roaming_app_data(), local_app_data()] {
        let folder = folder.expect("the shell knows the folder");
        assert!(folder.is_absolute(), "{folder:?}");
        assert!(folder.is_dir(), "{folder:?}");
    }
}
