//! `cargo xtask new-service`: copies the witness service `services/example` (aggregate `Note`) under a new name
//! (ADR-0010). The witness passes every gate, so the copy starts green, and there is no template to drift from it.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

const WITNESS: &str = "example";

pub(crate) fn new_service(root: &Path, service: &str, aggregate: &str, out: &mut impl Write) -> Result<()> {
    validate(service, aggregate)?;
    let from = root.join("services").join(WITNESS);
    let to = root.join("services").join(service);
    if to.exists() {
        bail!("services/{service} already exists; the generator never overwrites code");
    }
    for entry in walkdir::WalkDir::new(&from)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target")
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(&from)?.to_string_lossy().replace('\\', "/");
        let dest = to.join(rename(&rel, service, aggregate));
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = fs::read_to_string(entry.path()).with_context(|| format!("read {}", entry.path().display()))?;
        fs::write(&dest, rename(&text, service, aggregate))?;
        writeln!(out, "wrote {}", dest.strip_prefix(root)?.display())?;
    }
    // Renaming changes line lengths, so the copy is formatted before anyone commits it.
    let formatted = Command::new("cargo")
        .args(["fmt", "--all"])
        .current_dir(root)
        .status()?;
    if !formatted.success() {
        bail!("the service was written but `cargo fmt --all` failed; run it and read why");
    }
    Ok(())
}

/// One pass over `text`: what has been written is never matched again, so a new name that contains a witness word
/// (`notes`, `Footnote`) survives intact.
/// shortcut: a plural is the singular plus "s"; an irregular one needs a hand fix after generating.
pub(crate) fn rename(text: &str, service: &str, aggregate: &str) -> String {
    let snake_service = format!("{}_", service.replace('-', "_"));
    let kebab_service = format!("{service}-");
    let snake_aggregate = snake(aggregate);
    let words = [
        ("example_", snake_service.as_str()),
        ("example-", kebab_service.as_str()),
        ("Note", aggregate),
        ("note", snake_aggregate.as_str()),
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some((from, to)) = words.iter().find(|(from, _)| rest.starts_with(from)) {
            out.push_str(to);
            rest = rest.get(from.len()..).unwrap_or_default();
        } else {
            out.push(c);
            rest = rest.get(c.len_utf8()..).unwrap_or_default();
        }
    }
    out
}

pub(crate) fn validate(service: &str, aggregate: &str) -> Result<()> {
    let kebab = service.starts_with(|c: char| c.is_ascii_lowercase())
        && !service.ends_with('-')
        && service
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !kebab {
        bail!("the service name must be lowercase kebab-case, like `billing` or `order-intake`");
    }
    if service == WITNESS {
        bail!("`{WITNESS}` is the witness service every new one is copied from");
    }
    let pascal =
        aggregate.starts_with(|c: char| c.is_ascii_uppercase()) && aggregate.chars().all(|c| c.is_ascii_alphanumeric());
    if !pascal {
        bail!("the aggregate must be PascalCase, like `Invoice` or `OrderLine`");
    }
    Ok(())
}

fn snake(pascal: &str) -> String {
    let mut out = String::new();
    for (i, c) in pascal.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests;
