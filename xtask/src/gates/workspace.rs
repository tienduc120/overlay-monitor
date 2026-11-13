//! The workspace as the gates see it: each member crate, where it lives, and what it depends on.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use cargo_metadata::{DependencyKind, MetadataCommand};

/// Where a crate lives decides its kind (architecture.toml).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Location {
    /// `services/<service>/<layer>`; `service` keeps the whole path prefix so staged fixtures stay apart.
    Layer {
        service: String,
        layer: String,
    },
    Platform,
    Tool,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dep {
    pub(crate) name: String,
    pub(crate) dev: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Crate {
    pub(crate) name: String,
    /// Manifest directory relative to the workspace root, with forward slashes.
    pub(crate) rel: String,
    pub(crate) deps: Vec<Dep>,
    /// The crate root of its library target, if it has one.
    pub(crate) lib: Option<PathBuf>,
    /// Whether that root starts with `#![no_std]`.
    pub(crate) no_std: bool,
}

impl Crate {
    pub(crate) fn location(&self) -> Location {
        let parts: Vec<&str> = self.rel.split('/').collect();
        match parts.as_slice() {
            [prefix @ .., "services", service, layer] => {
                let mut path: Vec<&str> = prefix.to_vec();
                path.extend(["services", service]);
                Location::Layer {
                    service: path.join("/"),
                    layer: (*layer).to_owned(),
                }
            }
            [.., "platform", _] => Location::Platform,
            // Checked after the layers, so `services/<svc>/xtask` is a (wrong) layer, not the tool.
            [.., "xtask"] => Location::Tool,
            _ => Location::Unknown,
        }
    }
}

/// Reads the workspace members with `cargo metadata --no-deps`, which needs no network and no build.
pub(crate) fn load(root: &Path) -> Result<Vec<Crate>> {
    let meta = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
        .context("cargo metadata")?;
    let ws_root = meta.workspace_root.as_std_path();
    let mut crates = Vec::new();
    for pkg in meta.workspace_packages() {
        let dir = pkg.manifest_path.parent().map_or(ws_root, |d| d.as_std_path());
        let rel = dir
            .strip_prefix(ws_root)
            .unwrap_or(dir)
            .to_string_lossy()
            .replace('\\', "/");
        let lib = pkg
            .targets
            .iter()
            .find(|t| t.is_lib())
            .map(|t| t.src_path.clone().into_std_path_buf());
        let no_std = match &lib {
            Some(path) => starts_no_std(path)?,
            None => false,
        };
        let deps = pkg
            .dependencies
            .iter()
            .map(|d| Dep {
                name: d.name.clone(),
                dev: d.kind == DependencyKind::Development,
            })
            .collect();
        crates.push(Crate {
            name: pkg.name.to_string(),
            rel,
            deps,
            lib,
            no_std,
        });
    }
    Ok(crates)
}

fn starts_no_std(path: &Path) -> Result<bool> {
    let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let file = syn::parse_file(&src).with_context(|| format!("parse {}", path.display()))?;
    Ok(file.attrs.iter().any(|a| a.path().is_ident("no_std")))
}

#[cfg(test)]
mod tests;
