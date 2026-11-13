//! The gates: code that judges changes. Protected once committed (AGENTS.md, "Agent guard").

pub(crate) mod arch;
pub(crate) mod commitmsg;
pub(crate) mod guard;
pub(crate) mod manifest;
pub(crate) mod rules;
pub(crate) mod source;
pub(crate) mod verify;
pub(crate) mod workspace;

use std::fmt;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};

/// One thing a gate reports. `rule` is the id a fixture in `fixtures/violations/<id>` proves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) path: String,
    pub(crate) rule: &'static str,
    pub(crate) message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} [{}]", self.path, self.message, self.rule)
    }
}

/// Runs xtask's own gates (layer rules, manifest policy, source rules) over the workspace at `root`.
pub(crate) fn findings(root: &Path) -> Result<Vec<Finding>> {
    let config: arch::Config =
        toml::from_str(&fs::read_to_string(root.join("architecture.toml")).context("read architecture.toml")?)
            .context("parse architecture.toml")?;
    let crates = workspace::load(root)?;
    let mut found = arch::check(&config, &crates);
    for c in &crates {
        let dir = root.join(&c.rel);
        let manifest_path = dir.join("Cargo.toml");
        let manifest: toml::Table = toml::from_str(&fs::read_to_string(&manifest_path)?)
            .with_context(|| format!("parse {}", manifest_path.display()))?;
        found.extend(manifest::check(c, &manifest));
        found.extend(source::check(c, &rust_files(root, &dir)?));
    }
    Ok(found)
}

/// Every `.rs` file of a crate as (path relative to `root`, source).
fn rust_files(root: &Path, dir: &Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
    {
        let entry = entry?;
        if entry.path().extension().is_some_and(|e| e == "rs") {
            let rel = entry.path().strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            files.push((rel, fs::read_to_string(entry.path())?));
        }
    }
    Ok(files)
}
