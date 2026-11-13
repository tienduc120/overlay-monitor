use super::*;

#[test]
fn the_current_user_has_a_sid_string() {
    let sid = current_user_sid().expect("the process token has a user");
    assert!(sid.starts_with("S-1-5-"), "{sid}");
}
