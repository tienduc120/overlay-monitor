//! Layer rules from architecture.toml, checked against the workspace's real dependency graph.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::Finding;
use super::workspace::{Crate, Dep, Location};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) kinds: BTreeMap<String, Kind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Kind {
    #[serde(default)]
    pub(crate) layers: Vec<String>,
    #[serde(default)]
    pub(crate) platform: Vec<String>,
    pub(crate) allow: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) deny: Vec<String>,
    #[serde(default)]
    pub(crate) no_std: bool,
}

pub(crate) fn check(config: &Config, crates: &[Crate]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for c in crates {
        let Some((kind_name, kind)) = kind_of(config, c) else {
            let message = "a crate lives in services/<service>/<layer> (layers: see architecture.toml), \
                           platform/<name>, or xtask"
                .to_owned();
            findings.push(Finding {
                path: c.rel.clone(),
                rule: "arch-location",
                message,
            });
            continue;
        };
        if kind.no_std && !c.no_std {
            let message = format!("{} is {kind_name} code: its lib.rs must start with #![no_std]", c.name);
            findings.push(Finding {
                path: c.rel.clone(),
                rule: "arch-no-std",
                message,
            });
        }
        for dep in &c.deps {
            let found = match crates.iter().find(|m| m.name == dep.name) {
                Some(target) => internal_edge(c, kind_name, kind, target),
                None => external_edge(c, kind_name, kind, dep),
            };
            findings.extend(found.map(|(rule, message)| Finding {
                path: c.rel.clone(),
                rule,
                message,
            }));
        }
    }
    findings
}

fn kind_of<'a>(config: &'a Config, c: &Crate) -> Option<(&'a str, &'a Kind)> {
    let name = match c.location() {
        // "platform" and "tool" are kinds of place, not layers a service can have.
        Location::Layer { layer, .. } if layer == "platform" || layer == "tool" => return None,
        Location::Layer { layer, .. } => layer,
        Location::Platform => "platform".to_owned(),
        Location::Tool => "tool".to_owned(),
        Location::Unknown => return None,
    };
    config.kinds.get_key_value(&name).map(|(k, v)| (k.as_str(), v))
}

fn internal_edge(c: &Crate, kind_name: &str, kind: &Kind, target: &Crate) -> Option<(&'static str, String)> {
    let edge = format!("{} must not depend on {}", c.name, target.name);
    match (c.location(), target.location()) {
        (_, Location::Tool) => Some(("arch-tool", format!("{edge}: nothing depends on the tool"))),
        (Location::Layer { service: from, .. }, Location::Layer { service: to, .. }) if from != to => Some((
            "arch-cross-service",
            format!("{edge}: a service never depends on another service; share through platform or a contract"),
        )),
        (Location::Layer { .. }, Location::Layer { layer, .. }) if !kind.layers.contains(&layer) => Some((
            "arch-layer",
            format!(
                "{edge}: {kind_name} may depend only on [{}] of its own service",
                kind.layers.join(", ")
            ),
        )),
        (Location::Platform | Location::Tool | Location::Unknown, Location::Layer { .. }) => Some((
            "arch-layer",
            format!("{edge}: {kind_name} code knows nothing of any service"),
        )),
        (_, Location::Platform) if !kind.platform.iter().any(|p| p == "*" || *p == target.name) => Some((
            "arch-platform",
            format!(
                "{edge}: {kind_name} may use only [{}] from platform",
                kind.platform.join(", ")
            ),
        )),
        (_, Location::Layer { .. } | Location::Platform | Location::Unknown) => None,
    }
}

fn external_edge(c: &Crate, kind_name: &str, kind: &Kind, dep: &Dep) -> Option<(&'static str, String)> {
    if dep.dev {
        return None;
    }
    let edge = format!("{} must not depend on {}", c.name, dep.name);
    if let Some(allow) = &kind.allow
        && !allow.contains(&dep.name)
    {
        return Some((
            "arch-external-allow",
            format!(
                "{edge}: {kind_name} may use only [{}]; a new one is a decision (docs/adr)",
                allow.join(", ")
            ),
        ));
    }
    kind.deny.contains(&dep.name).then(|| {
        (
            "arch-external-deny",
            format!("{edge}: architecture.toml denies it for {kind_name}"),
        )
    })
}

#[cfg(test)]
mod tests;
