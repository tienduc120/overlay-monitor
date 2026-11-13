use super::check;
use crate::gates::workspace::Crate;

fn at(rel: &str, name: &str) -> Crate {
    Crate {
        name: name.to_owned(),
        rel: rel.to_owned(),
        deps: vec![],
        lib: None,
        no_std: false,
    }
}

const GOOD_PACKAGE: &str = r#"
[package]
name = "billing-application"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true
"#;

fn rules(c: &Crate, manifest: &str) -> Vec<&'static str> {
    check(c, &toml::from_str(manifest).unwrap())
        .into_iter()
        .map(|f| f.rule)
        .collect()
}

fn application() -> Crate {
    at("services/billing/application", "billing-application")
}

#[test]
fn a_manifest_that_inherits_everything_has_no_findings() {
    let manifest = format!(
        "{GOOD_PACKAGE}\n[dependencies]\nbilling-domain = {{ path = \"../domain\" }}\nthiserror = {{ workspace = true }}\n\
         [dev-dependencies]\ntokio = {{ workspace = true, features = [\"macros\"] }}\n"
    );

    assert_eq!(rules(&application(), &manifest), Vec::<&str>::new());
}

#[test]
fn a_crate_must_take_the_workspace_lints() {
    let manifest = GOOD_PACKAGE.replace("[lints]\nworkspace = true", "[lints.clippy]\nunwrap_used = \"allow\"");

    assert_eq!(rules(&application(), &manifest), vec!["manifest-lints"]);
}

#[test]
fn package_fields_come_from_the_workspace() {
    let manifest = GOOD_PACKAGE.replace("edition.workspace = true", "edition = \"2021\"");

    let findings = check(&application(), &toml::from_str(&manifest).unwrap());

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule, "manifest-inherit");
    assert!(findings[0].message.contains("edition"), "{}", findings[0].message);
}

#[test]
fn a_package_is_named_after_where_it_lives() {
    let c = at("services/billing/application", "billing-app");

    assert_eq!(
        rules(&c, &GOOD_PACKAGE.replace("billing-application", "billing-app")),
        vec!["manifest-name"]
    );
    let platform = at("platform/clock", "clock");
    assert_eq!(
        rules(&platform, &GOOD_PACKAGE.replace("billing-application", "clock")),
        vec!["manifest-name"]
    );
}

#[test]
fn an_external_version_belongs_in_the_workspace_not_the_member() {
    let manifest =
        format!("{GOOD_PACKAGE}\n[dependencies]\nserde = \"1\"\n[dev-dependencies]\nrand = {{ version = \"0.10\" }}\n");

    assert_eq!(
        rules(&application(), &manifest),
        vec!["manifest-dependency", "manifest-dependency"]
    );
}

#[test]
fn a_path_dependency_may_only_reach_a_sibling_layer_of_the_same_service() {
    let manifest = format!(
        "{GOOD_PACKAGE}\n[dependencies]\nplatform-clock = {{ path = \"../../../platform/clock\" }}\n\
         orders-domain = {{ path = \"../../orders/domain\" }}\n"
    );

    assert_eq!(
        rules(&application(), &manifest),
        vec!["manifest-dependency", "manifest-dependency"]
    );
}

#[test]
fn target_specific_dependencies_are_held_to_the_same_rule() {
    let manifest = format!("{GOOD_PACKAGE}\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n");

    assert_eq!(rules(&application(), &manifest), vec!["manifest-dependency"]);
}
