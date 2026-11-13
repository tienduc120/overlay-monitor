use super::{is_refactor, refactor_problems, validate};

#[test]
fn a_refactor_is_recognised_with_a_scope_or_a_breaking_mark() {
    for msg in [
        "refactor: tidy",
        "refactor(core): tidy",
        "refactor!: tidy",
        "refactor(core)!: tidy",
    ] {
        assert!(is_refactor(msg), "{msg}");
    }
    for msg in [
        "feat: refactor the core",
        "refactors: tidy",
        "# refactor: a comment line\nfix: x",
    ] {
        assert!(!is_refactor(msg), "{msg}");
    }
}

#[test]
fn a_conventional_message_is_valid() {
    assert_eq!(
        validate("feat(billing): add invoices\n\nWhy it matters.\n"),
        Vec::<String>::new()
    );
    assert_eq!(
        validate("fix: stop the leak\n# a git comment line\n"),
        Vec::<String>::new()
    );
}

#[test]
fn merge_and_revert_messages_from_git_are_left_alone() {
    assert_eq!(validate("Merge branch 'main' into x"), Vec::<String>::new());
    assert_eq!(validate("Revert \"feat: add x\""), Vec::<String>::new());
}

#[test]
fn every_problem_is_reported_at_once() {
    let problems = validate("Feature(Billing): Added invoices.\nbody without a blank line");

    assert_eq!(problems.len(), 5, "{problems:#?}");
}

#[test]
fn the_first_line_must_have_the_type_scope_subject_shape() {
    assert_eq!(validate("add invoices").len(), 1);
    assert_eq!(validate("").len(), 1);
}

#[test]
fn the_first_line_has_a_length_limit() {
    let long = format!("feat: {}", "a".repeat(70));

    assert_eq!(validate(&long).len(), 1);
}

#[test]
fn a_refactor_may_add_test_lines_but_not_change_them() {
    let added_only = "diff --git a/x/src/a/tests.rs b/x/src/a/tests.rs\n--- a/x/src/a/tests.rs\n+++ b/x/src/a/tests.rs\n\
                      @@ -3,0 +4,2 @@\n+#[test]\n+fn new_one() {}\n";
    let changed = "diff --git a/x/src/a/tests.rs b/x/src/a/tests.rs\n--- a/x/src/a/tests.rs\n+++ b/x/src/a/tests.rs\n\
                   @@ -3 +3 @@\n-    assert_eq!(f(), 1);\n+    assert_eq!(f(), 2);\n";

    assert_eq!(refactor_problems(added_only), Vec::<String>::new());
    assert_eq!(refactor_problems(changed).len(), 1);
}

#[test]
fn removed_blank_lines_comments_and_production_code_are_fine_in_a_refactor() {
    let diff = "--- a/x/tests/it.rs\n+++ b/x/tests/it.rs\n@@ -1,2 +0,0 @@\n-\n-// old note\n\
                --- a/x/src/lib.rs\n+++ b/x/src/lib.rs\n@@ -1 +1 @@\n-pub fn f() -> u8 { 1 }\n+pub fn f() -> u8 { 2 - 1 }\n";

    assert_eq!(refactor_problems(diff), Vec::<String>::new());
}
