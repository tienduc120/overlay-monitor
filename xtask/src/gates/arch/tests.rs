use super::{Config, check};
use crate::gates::workspace::{Crate, Dep};

// The real rules: a test here breaks when architecture.toml changes, which is the point of protecting it.
fn config() -> Config {
    toml::from_str(include_str!("../../../../architecture.toml")).unwrap()
}

fn krate(rel: &str, name: &str, deps: &[&str]) -> Crate {
    let deps = deps
        .iter()
        .map(|d| Dep {
            name: (*d).to_owned(),
            dev: false,
        })
        .collect();
    let no_std = rel.ends_with("/domain");
    Crate {
        name: name.to_owned(),
        rel: rel.to_owned(),
        deps,
        lib: None,
        no_std,
    }
}

fn service(svc: &str) -> Vec<Crate> {
    let p = format!("services/{svc}");
    vec![
        krate(&format!("{p}/domain"), &format!("{svc}-domain"), &["thiserror", "jiff"]),
        krate(
            &format!("{p}/application"),
            &format!("{svc}-application"),
            &[&format!("{svc}-domain"), "platform-clock"],
        ),
        krate(
            &format!("{p}/infrastructure"),
            &format!("{svc}-infrastructure"),
            &[&format!("{svc}-domain"), &format!("{svc}-application"), "tokio"],
        ),
        krate(
            &format!("{p}/interfaces"),
            &format!("{svc}-interfaces"),
            &[&format!("{svc}-application"), "axum"],
        ),
        krate(
            &format!("{p}/api"),
            &format!("{svc}-api"),
            &[
                &format!("{svc}-infrastructure"),
                &format!("{svc}-interfaces"),
                "platform-http",
            ],
        ),
        krate("platform/clock", "platform-clock", &["jiff"]),
        krate("platform/http", "platform-http", &["axum"]),
        krate("xtask", "xtask", &["anyhow"]),
    ]
}

fn rules(crates: &[Crate]) -> Vec<&'static str> {
    check(&config(), crates).into_iter().map(|f| f.rule).collect()
}

fn with_dep(mut crates: Vec<Crate>, name: &str, dep: &str, dev: bool) -> Vec<Crate> {
    let c = crates.iter_mut().find(|c| c.name == name).unwrap();
    c.deps.push(Dep {
        name: dep.to_owned(),
        dev,
    });
    crates
}

#[test]
fn a_service_that_follows_the_rules_has_no_findings() {
    assert_eq!(check(&config(), &service("billing")), vec![]);
}

#[test]
fn the_domain_may_not_depend_on_its_own_infrastructure() {
    let crates = with_dep(service("billing"), "billing-domain", "billing-infrastructure", false);

    let findings = check(&config(), &crates);

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule, "arch-layer");
    assert!(
        findings[0].message.contains("billing-infrastructure"),
        "names the edge: {}",
        findings[0].message
    );
}

#[test]
fn interfaces_may_not_reach_past_the_application_into_the_domain() {
    let crates = with_dep(service("billing"), "billing-interfaces", "billing-domain", false);

    assert_eq!(rules(&crates), vec!["arch-layer"]);
}

#[test]
fn a_test_only_edge_still_obeys_the_layers() {
    let crates = with_dep(service("billing"), "billing-interfaces", "billing-domain", true);

    assert_eq!(rules(&crates), vec!["arch-layer"]);
}

#[test]
fn a_service_never_depends_on_another_service() {
    let mut crates = service("billing");
    crates.extend(service("orders").into_iter().filter(|c| c.rel.starts_with("services/")));
    let crates = with_dep(crates, "orders-application", "billing-domain", false);

    assert_eq!(rules(&crates), vec!["arch-cross-service"]);
}

#[test]
fn platform_code_knows_nothing_of_any_service() {
    let crates = with_dep(service("billing"), "platform-http", "billing-application", false);

    assert_eq!(rules(&crates), vec!["arch-layer"]);
}

#[test]
fn a_layer_may_use_only_the_platform_crates_its_kind_allows() {
    let crates = with_dep(service("billing"), "billing-application", "platform-http", false);

    assert_eq!(rules(&crates), vec!["arch-platform"]);
}

#[test]
fn nothing_may_depend_on_the_tool() {
    let crates = with_dep(service("billing"), "platform-clock", "xtask", false);

    assert_eq!(rules(&crates), vec!["arch-tool"]);
}

#[test]
fn the_domain_may_use_only_its_allowlisted_external_crates() {
    let crates = with_dep(service("billing"), "billing-domain", "serde", false);

    let findings = check(&config(), &crates);

    assert_eq!(
        findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
        vec!["arch-external-allow"]
    );
    assert!(findings[0].message.contains("serde"), "{}", findings[0].message);
}

#[test]
fn an_external_test_crate_is_free_for_dev_dependencies() {
    let crates = with_dep(service("billing"), "billing-domain", "proptest", true);

    assert_eq!(rules(&crates), Vec::<&str>::new());
}

#[test]
fn interfaces_may_not_touch_the_database_driver() {
    let crates = with_dep(service("billing"), "billing-interfaces", "sqlx", false);

    assert_eq!(rules(&crates), vec!["arch-external-deny"]);
}

#[test]
fn a_domain_crate_must_be_no_std() {
    let mut crates = service("billing");
    crates[0].no_std = false;

    assert_eq!(rules(&crates), vec!["arch-no-std"]);
}

#[test]
fn a_crate_outside_the_known_places_or_with_an_unknown_layer_is_reported() {
    let mut crates = service("billing");
    crates.push(krate("libs/helper", "helper", &[]));
    crates.push(krate("services/billing/widgets", "billing-widgets", &[]));

    assert_eq!(rules(&crates), vec!["arch-location", "arch-location"]);
}
