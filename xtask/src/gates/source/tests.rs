use super::check;
use crate::gates::workspace::Crate;

#[test]
fn passing_restore_as_a_function_value_is_caught_too() {
    let call = "fn f(ids: Vec<NoteId>) -> Vec<Note> { ids.into_iter().map(Note::restore).collect() }\n";
    let application = at("services/billing/application", false);

    assert_eq!(
        rules(&application, &[("services/billing/application/src/a.rs", call)]),
        vec!["source-restore"]
    );
}

#[test]
fn a_test_only_helper_may_rebuild_an_aggregate() {
    let helper = "#[cfg(test)]\nfn sample() -> Note { Note::restore(id(), name()) }\n";
    let application = at("services/billing/application", false);

    assert_eq!(
        rules(&application, &[("services/billing/application/src/a.rs", helper)]),
        Vec::<&str>::new()
    );
}

#[test]
fn only_platform_win32_may_expect_unsafe_code() {
    let src = "#[expect(unsafe_code, reason = \"ffi\")]\npub fn f() {}\n";
    let other = named("platform-clock", "platform/clock");
    let win32 = named("platform-win32", "platform/win32");

    assert_eq!(
        rules(&other, &[("platform/clock/src/lib.rs", src)]),
        vec!["source-unsafe-scope"]
    );
    assert_eq!(rules(&win32, &[("platform/win32/src/lib.rs", src)]), Vec::<&str>::new());
}

#[test]
fn allowing_unsafe_code_is_refused_outside_platform_win32_too() {
    let src = "#[allow(unsafe_code)]\npub fn f() {}\n";
    let domain = named("billing-domain", "services/billing/domain");

    assert!(rules(&domain, &[("services/billing/domain/src/lib.rs", src)]).contains(&"source-unsafe-scope"));
}

fn named(name: &str, rel: &str) -> Crate {
    Crate {
        name: name.to_owned(),
        ..at(rel, false)
    }
}

fn at(rel: &str, no_std: bool) -> Crate {
    Crate {
        name: "x".to_owned(),
        rel: rel.to_owned(),
        deps: vec![],
        lib: None,
        no_std,
    }
}

fn rules(c: &Crate, files: &[(&str, &str)]) -> Vec<&'static str> {
    let files: Vec<(String, String)> = files.iter().map(|(p, s)| ((*p).to_owned(), (*s).to_owned())).collect();
    check(c, &files).into_iter().map(|f| f.rule).collect()
}

const NOTE: &str = r"
#![no_std]
extern crate alloc;
use alloc::string::String;

pub struct NoteId(String);

/// A note someone wrote. agent:aggregate
pub struct Note { id: NoteId, name: String }

impl Note {
    pub fn new(id: NoteId, name: String) -> Result<Self, ()> { Ok(Self { id, name }) }
    pub fn restore(id: NoteId, name: String) -> Self { Self { id, name } }
}

#[cfg(test)]
mod tests;
";

fn domain() -> Crate {
    at("services/billing/domain", true)
}

#[test]
fn a_well_shaped_aggregate_in_a_no_std_domain_has_no_findings() {
    assert_eq!(
        rules(&domain(), &[("services/billing/domain/src/lib.rs", NOTE)]),
        Vec::<&str>::new()
    );
}

#[test]
fn an_aggregate_id_must_be_a_type_the_crate_defines() {
    let src = NOTE.replace("id: NoteId, name", "id: String, name");

    assert_eq!(
        rules(&domain(), &[("services/billing/domain/src/lib.rs", &src)]),
        vec!["source-aggregate"]
    );
}

#[test]
fn an_aggregate_needs_both_constructors() {
    let src = NOTE.replace("pub fn restore(", "pub fn rebuild(");

    let findings = check(&domain(), &[("services/billing/domain/src/lib.rs".to_owned(), src)]);

    assert_eq!(
        findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
        vec!["source-aggregate"]
    );
    assert!(findings[0].message.contains("restore"), "{}", findings[0].message);
}

#[test]
fn only_infrastructure_rebuilds_an_aggregate_with_restore() {
    let call = "fn f() { let _n = domain::Note::restore(id(), name()); }\n";
    let application = at("services/billing/application", false);
    let infrastructure = at("services/billing/infrastructure", false);

    let findings = check(
        &application,
        &[("services/billing/application/src/a.rs".to_owned(), call.to_owned())],
    );

    assert_eq!(
        findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
        vec!["source-restore"]
    );
    assert_eq!(
        findings[0].path, "services/billing/application/src/a.rs:1",
        "the finding points at the line"
    );
    assert_eq!(
        rules(&infrastructure, &[("services/billing/infrastructure/src/a.rs", call)]),
        Vec::<&str>::new()
    );
}

#[test]
fn a_test_file_may_rebuild_an_aggregate() {
    let call = "fn f() { let _n = Note::restore(id(), name()); }\n";
    let application = at("services/billing/application", false);

    assert_eq!(
        rules(&application, &[("services/billing/application/src/a/tests.rs", call)]),
        Vec::<&str>::new()
    );
    assert_eq!(
        rules(&application, &[("services/billing/application/tests/it.rs", call)]),
        Vec::<&str>::new()
    );
}

#[test]
fn a_no_std_crate_may_not_bring_std_back_outside_tests() {
    let with_std = NOTE.replace("extern crate alloc;", "extern crate alloc;\nextern crate std;");
    let test_std = NOTE.replace(
        "extern crate alloc;",
        "extern crate alloc;\n#[cfg(test)]\nextern crate std;",
    );

    assert_eq!(
        rules(&domain(), &[("services/billing/domain/src/lib.rs", &with_std)]),
        vec!["source-extern-std"]
    );
    assert_eq!(
        rules(&domain(), &[("services/billing/domain/src/lib.rs", &test_std)]),
        Vec::<&str>::new()
    );
}

#[test]
fn unit_tests_live_in_a_sibling_file_not_inline() {
    let inline = "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
    let platform = at("platform/clock", false);

    assert_eq!(
        rules(&platform, &[("platform/clock/src/lib.rs", inline)]),
        vec!["source-inline-tests"]
    );
}

#[test]
fn a_crate_wide_allow_is_reported() {
    let platform = at("platform/clock", false);

    let found = rules(
        &platform,
        &[("platform/clock/src/lib.rs", "#![allow(dead_code)]\npub fn f() {}\n")],
    );

    assert_eq!(found, vec!["source-inner-allow"]);
}

#[test]
fn a_file_that_does_not_parse_is_a_finding_not_a_pass() {
    let platform = at("platform/clock", false);

    assert_eq!(
        rules(&platform, &[("platform/clock/src/lib.rs", "pub fn f( {")]),
        vec!["source-parse"]
    );
}
