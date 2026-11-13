//! Proves every rule fires on a deliberately bad fixture and stays silent on a good one (ADR-0002). A rule that
//! nothing proves is advice, not a rule.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use toml_edit::{DocumentMut, Item};

/// `fixtures/violations/<id>/rule.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Spec {
    /// `clippy` (rustc and clippy diagnostics) or `gates` (xtask's own: arch, manifest, source).
    pub(crate) enforcer: String,
    /// Text a finding on this fixture must contain; it pins the rule that fired, not just any finding.
    pub(crate) contains: String,
    /// What the rule protects, for a person reading the fixture.
    pub(crate) doc: String,
}

/// One finding from an enforcer, with its path relative to the staged workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Diag {
    pub(crate) enforcer: String,
    pub(crate) path: String,
    pub(crate) text: String,
}

#[derive(Debug)]
pub(crate) struct Outcome {
    pub(crate) id: String,
    pub(crate) pass: bool,
    pub(crate) detail: String,
}

const CLEAN: &str = "clean";
/// Configuration the enforcers read. Without it they would run on their defaults and prove nothing about ours.
const CONFIGS: [&str; 5] = [
    "clippy.toml",
    "rustfmt.toml",
    "architecture.toml",
    "rust-toolchain.toml",
    "Cargo.lock",
];

/// Stages every fixture and the clean twin as one throwaway workspace, runs each enforcer once, and reports per
/// fixture whether its rule fired.
pub(crate) fn run(root: &Path, out: &mut impl Write) -> Result<bool> {
    let specs = load_specs(root)?;
    let stage = stage(root)?;
    let mut diags: Vec<Diag> = super::findings(&stage)?
        .into_iter()
        .map(|f| Diag {
            enforcer: "gates".to_owned(),
            path: f.path,
            text: format!("{}: {}", f.rule, f.message),
        })
        .collect();
    diags.extend(clippy(root, &stage)?);
    let outcomes = judge(&specs, &diags);
    for o in &outcomes {
        let mark = if o.pass { "ok  " } else { "FAIL" };
        writeln!(
            out,
            "{mark}  {}{}",
            o.id,
            if o.pass {
                String::new()
            } else {
                format!("\n      {}", o.detail)
            }
        )?;
    }
    Ok(outcomes.iter().all(|o| o.pass))
}

fn load_specs(root: &Path) -> Result<Vec<(String, Spec)>> {
    let mut specs = Vec::new();
    for entry in fs::read_dir(root.join("fixtures/violations")).context("read fixtures/violations")? {
        let dir = entry?.path();
        let id = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(dir.join("rule.toml")).with_context(|| format!("{id}: read rule.toml"))?;
        specs.push((
            id.clone(),
            toml::from_str(&text).with_context(|| format!("{id}: parse rule.toml"))?,
        ));
    }
    specs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(specs)
}

fn stage(root: &Path) -> Result<PathBuf> {
    let stage = root.join(".xtask/stage");
    if stage.exists() {
        fs::remove_dir_all(&stage).context("clear .xtask/stage")?;
    }
    let mut members = Vec::new();
    let violations = fs::read_dir(root.join("fixtures/violations"))?.map(|e| e.map(|e| e.path()));
    for dir in violations.chain([Ok(root.join("fixtures/clean"))]) {
        let dir = dir?;
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        copy_tree(&dir, &stage.join(&name), &stage, &mut members)?;
    }
    for config in CONFIGS {
        if root.join(config).exists() {
            fs::copy(root.join(config), stage.join(config)).with_context(|| format!("stage {config}"))?;
        }
    }
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    fs::write(stage.join("Cargo.toml"), staged_manifest(root, &manifest, &members)?)?;
    Ok(stage)
}

fn copy_tree(from: &Path, to: &Path, stage: &Path, members: &mut Vec<String>) -> Result<()> {
    for entry in walkdir::WalkDir::new(from)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
    {
        let entry = entry?;
        let dest = to.join(entry.path().strip_prefix(from)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
            if entry.file_name() == "Cargo.toml"
                && let Some(dir) = dest.parent()
            {
                members.push(dir.strip_prefix(stage)?.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

/// The repository's workspace table with the fixtures as members and platform paths made absolute.
fn staged_manifest(root: &Path, manifest: &str, members: &[String]) -> Result<String> {
    let mut doc: DocumentMut = manifest.parse().context("parse the root Cargo.toml")?;
    let workspace = doc
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .context("the root Cargo.toml has no [workspace]")?;
    workspace.insert(
        "members",
        toml_edit::value(members.iter().collect::<toml_edit::Array>()),
    );
    if let Some(deps) = workspace.get_mut("dependencies").and_then(Item::as_table_like_mut) {
        for (_, spec) in deps.iter_mut() {
            let Some(spec) = spec.as_table_like_mut() else { continue };
            if let Some(path) = spec.get("path").and_then(Item::as_str).map(str::to_owned) {
                let absolute = root.join(path).to_string_lossy().replace('\\', "/");
                spec.insert("path", toml_edit::value(absolute));
            }
        }
    }
    Ok(doc.to_string())
}

fn clippy(root: &Path, stage: &Path) -> Result<Vec<Diag>> {
    let output = Command::new("cargo")
        .args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--keep-going",
            "--message-format=json",
            "--quiet",
        ])
        .current_dir(stage)
        .env("CARGO_TARGET_DIR", root.join("target/rules"))
        .output()
        .context("run cargo clippy on the staged fixtures")?;
    let diags = parse_clippy(&String::from_utf8_lossy(&output.stdout));
    if diags.is_empty() && !output.status.success() {
        bail!(
            "cargo clippy failed before reporting anything:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(diags)
}

pub(crate) fn parse_clippy(out: &str) -> Vec<Diag> {
    out.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| v.get("reason").and_then(Value::as_str) == Some("compiler-message"))
        .filter_map(|v| {
            let message = v.get("message")?;
            let spans = message.get("spans")?.as_array()?;
            let primary = spans
                .iter()
                .find(|s| s.get("is_primary").and_then(Value::as_bool) == Some(true))?;
            let path = primary.get("file_name")?.as_str()?.replace('\\', "/");
            let text = message.get("message").and_then(Value::as_str).unwrap_or_default();
            let text = match message.get("code").and_then(|c| c.get("code")).and_then(Value::as_str) {
                Some(code) => format!("{code}: {text}"),
                None => text.to_owned(),
            };
            Some(Diag {
                enforcer: "clippy".to_owned(),
                path,
                text,
            })
        })
        .collect()
}

pub(crate) fn judge(specs: &[(String, Spec)], diags: &[Diag]) -> Vec<Outcome> {
    let under = |id: &str| {
        let prefix = format!("{id}/");
        diags
            .iter()
            .filter(move |d| d.path.starts_with(&prefix))
            .collect::<Vec<_>>()
    };
    let mut outcomes: Vec<Outcome> = specs
        .iter()
        .map(|(id, spec)| {
            let here = under(id);
            let pass = here
                .iter()
                .any(|d| d.enforcer == spec.enforcer && d.text.contains(&spec.contains));
            let detail = format!(
                "{} -- expected {} to report `{}` here; it reported: {}",
                spec.doc,
                spec.enforcer,
                spec.contains,
                seen(&here)
            );
            Outcome {
                id: id.clone(),
                pass,
                detail,
            }
        })
        .collect();
    let clean = under(CLEAN);
    let detail = format!("the clean twin must have no findings; it has: {}", seen(&clean));
    outcomes.push(Outcome {
        id: CLEAN.to_owned(),
        pass: clean.is_empty(),
        detail,
    });
    outcomes
}

fn seen(diags: &[&Diag]) -> String {
    if diags.is_empty() {
        return "nothing".to_owned();
    }
    diags
        .iter()
        .take(3)
        .map(|d| format!("[{}] {} {}", d.enforcer, d.path, d.text))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests;
