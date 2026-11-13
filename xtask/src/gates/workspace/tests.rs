use super::{Crate, Location};

fn at(rel: &str) -> Crate {
    Crate {
        name: "x".to_owned(),
        rel: rel.to_owned(),
        deps: vec![],
        lib: None,
        no_std: false,
    }
}

fn layer(service: &str, layer: &str) -> Location {
    Location::Layer {
        service: service.to_owned(),
        layer: layer.to_owned(),
    }
}

#[test]
fn a_layer_directory_named_like_the_tool_is_still_a_layer() {
    assert_eq!(
        at("services/billing/xtask").location(),
        layer("services/billing", "xtask")
    );
}

#[test]
fn a_crate_under_services_is_a_layer_of_that_service() {
    assert_eq!(
        at("services/billing/domain").location(),
        layer("services/billing", "domain")
    );
}

#[test]
fn a_staged_fixture_keeps_its_prefix_so_two_fixtures_never_share_a_service() {
    assert_eq!(
        at("arch-layer/services/billing/domain").location(),
        layer("arch-layer/services/billing", "domain"),
    );
}

#[test]
fn platform_and_the_tool_are_recognised_and_anything_else_is_unknown() {
    assert_eq!(at("platform/clock").location(), Location::Platform);
    assert_eq!(at("fx/platform/helper").location(), Location::Platform);
    assert_eq!(at("xtask").location(), Location::Tool);
    assert_eq!(
        at("arch-tool/xtask").location(),
        Location::Tool,
        "a staged fixture's tool"
    );
    assert_eq!(at("libs/helper").location(), Location::Unknown);
    assert_eq!(at("services/billing").location(), Location::Unknown);
}
