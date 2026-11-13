//! Manifest policy: every member inherits the workspace's lints, package fields, and dependency versions, and is
//! named after where it lives. One Cargo.toml decides; members only opt in.

use toml::{Table, Value};

use super::Finding;
use super::workspace::{Crate, Location};

const INHERITED: [&str; 5] = ["version", "edition", "rust-version", "license", "publish"];
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

pub(crate) fn check(c: &Crate, manifest: &Table) -> Vec<Finding> {
    let mut found: Vec<(&'static str, String)> = Vec::new();
    if !inherits(manifest.get("lints")) {
        found.push((
            "manifest-lints",
            "write `[lints]` with only `workspace = true`: lint levels are decided once".to_owned(),
        ));
    }
    let package = manifest.get("package").and_then(Value::as_table);
    let missing: Vec<&str> = INHERITED
        .into_iter()
        .filter(|k| !inherits(package.and_then(|p| p.get(*k))))
        .collect();
    if !missing.is_empty() {
        let message = format!(
            "set {} with `.workspace = true`: the workspace decides them",
            missing.join(", ")
        );
        found.push(("manifest-inherit", message));
    }
    if let Some(want) = expected_name(c)
        && want != c.name
    {
        found.push((
            "manifest-name",
            format!("the package at {} must be named `{want}`", c.rel),
        ));
    }
    for (name, spec) in dependencies(manifest) {
        if let Some(message) = dependency_problem(c, name, spec) {
            found.push(("manifest-dependency", message));
        }
    }
    let path = format!("{}/Cargo.toml", c.rel);
    found
        .into_iter()
        .map(|(rule, message)| Finding {
            path: path.clone(),
            rule,
            message,
        })
        .collect()
}

fn inherits(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_table)
        .and_then(|t| t.get("workspace"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn expected_name(c: &Crate) -> Option<String> {
    let last = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
    match c.location() {
        Location::Layer { service, layer } => Some(format!("{}-{layer}", last(&service))),
        Location::Platform => Some(format!("platform-{}", last(&c.rel))),
        Location::Tool => Some("xtask".to_owned()),
        Location::Unknown => None,
    }
}

/// Every dependency table, including the `[target.'cfg(..)'.*]` ones.
fn dependencies(manifest: &Table) -> Vec<(&str, &Value)> {
    let targets = manifest
        .get("target")
        .and_then(Value::as_table)
        .into_iter()
        .flat_map(Table::values);
    let roots = std::iter::once(manifest).chain(targets.filter_map(Value::as_table));
    roots
        .flat_map(|root| {
            DEPENDENCY_TABLES
                .into_iter()
                .filter_map(|t| root.get(t))
                .filter_map(Value::as_table)
        })
        .flat_map(|table| table.iter().map(|(name, spec)| (name.as_str(), spec)))
        .collect()
}

fn dependency_problem(c: &Crate, name: &str, spec: &Value) -> Option<String> {
    if inherits(Some(spec)) {
        return None;
    }
    let Some(path) = spec.as_table().and_then(|t| t.get("path")).and_then(Value::as_str) else {
        return Some(format!(
            "dependency `{name}` must come from the workspace: declare its version once in the root Cargo.toml \
             and write `{name} = {{ workspace = true }}` here"
        ));
    };
    let sibling = path
        .strip_prefix("../")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains(['/', '\\']));
    if sibling && matches!(c.location(), Location::Layer { .. }) {
        return None;
    }
    Some(format!(
        "path dependency `{name}` may only reach a sibling layer of this service (`../<layer>`); \
         platform crates come from the workspace"
    ))
}

#[cfg(test)]
mod tests;
