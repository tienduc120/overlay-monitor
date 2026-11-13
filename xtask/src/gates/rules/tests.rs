use super::{Diag, Spec, judge, parse_clippy};

fn spec(id: &str, enforcer: &str, contains: &str) -> (String, Spec) {
    (
        id.to_owned(),
        Spec {
            enforcer: enforcer.to_owned(),
            contains: contains.to_owned(),
            doc: String::new(),
        },
    )
}

fn diag(enforcer: &str, path: &str, text: &str) -> Diag {
    Diag {
        enforcer: enforcer.to_owned(),
        path: path.to_owned(),
        text: text.to_owned(),
    }
}

#[test]
fn clippy_json_becomes_one_diag_per_compiler_message_with_its_primary_file() {
    let out = r#"{"reason":"compiler-artifact","target":{}}
{"reason":"compiler-message","message":{"code":{"code":"clippy::unwrap_used"},"message":"used `unwrap()` on a `Result` value","spans":[{"file_name":"no-unwrap\\platform\\no-unwrap\\src\\lib.rs","is_primary":true}]}}
{"reason":"compiler-message","message":{"code":null,"message":"3 warnings emitted","spans":[]}}
not json at all
"#;

    let diags = parse_clippy(out);

    assert_eq!(
        diags,
        vec![diag(
            "clippy",
            "no-unwrap/platform/no-unwrap/src/lib.rs",
            "clippy::unwrap_used: used `unwrap()` on a `Result` value"
        )]
    );
}

#[test]
fn a_fixture_passes_only_when_its_own_enforcer_reports_the_expected_text_under_its_directory() {
    let specs = vec![
        spec("no-unwrap", "clippy", "clippy::unwrap_used"),
        spec("arch-layer", "gates", "arch-layer"),
    ];
    let diags = vec![
        diag(
            "clippy",
            "no-unwrap/platform/no-unwrap/src/lib.rs",
            "clippy::unwrap_used: used unwrap",
        ),
        diag(
            "clippy",
            "arch-layer/services/b/domain/Cargo.toml",
            "arch-layer: wrong enforcer",
        ),
    ];

    let outcomes = judge(&specs, &diags);

    assert_eq!(
        outcomes.iter().map(|o| (o.id.as_str(), o.pass)).collect::<Vec<_>>(),
        vec![("no-unwrap", true), ("arch-layer", false), ("clean", true),]
    );
}

#[test]
fn the_clean_twin_fails_on_any_finding_from_any_enforcer() {
    let diags = vec![diag(
        "gates",
        "clean/services/b/domain/src/lib.rs:3",
        "source-restore: ...",
    )];

    let outcomes = judge(&[], &diags);

    assert!(!outcomes[0].pass, "{outcomes:?}");
    assert!(
        outcomes[0].detail.contains("source-restore"),
        "says what fired: {}",
        outcomes[0].detail
    );
}

#[test]
fn a_fixture_whose_directory_shares_a_prefix_with_another_is_not_credited_with_its_findings() {
    let specs = vec![spec("arch", "gates", "arch-layer")];
    let diags = vec![diag(
        "gates",
        "arch-layer/services/b/domain/Cargo.toml",
        "arch-layer: x",
    )];

    assert!(!judge(&specs, &diags)[0].pass);
}
