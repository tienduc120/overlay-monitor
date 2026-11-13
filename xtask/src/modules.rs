//! `cargo xtask modules`: downloads the pinned PawnIO.Modules release, checks every hash, and copies the files the
//! app uses next to the debug and release builds. The files are LGPL-2.1 and never committed.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail, ensure};

const VERSION: &str = "0.2.11";
const URL: &str = "https://github.com/namazso/PawnIO.Modules/releases/download/0.2.11/release_0_2_11.zip";
const ZIP_NAME: &str = "release_0_2_11.zip";
const ZIP_SHA256: &str = "43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4";

/// Moving a pin is a decision: change the version, the zip hash, and these together.
const FILES: [(&str, &str); 5] = [
    (
        "AMDFamily17.bin",
        "dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91",
    ),
    (
        "IntelMSR.bin",
        "d6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f",
    ),
    (
        "SmbusPIIX4.bin",
        "91f9b4b1c39e3d399ce48477a89d8f6bd3e58a2241064a4daec3a1513dff56e5",
    ),
    (
        "SmbusI801.bin",
        "a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c",
    ),
    (
        "COPYING",
        "1e7e6bae5a5bde32f1ae5a7c37a082d1ab03cf89354f7f936ac40be9e39a6531",
    ),
];

/// Git Bash puts GNU tar first on `PATH`, which cannot read a zip; Windows ships bsdtar here.
const WINDOWS_TAR: &str = r"C:\Windows\System32\tar.exe";

pub(crate) fn run(root: &Path, out: &mut impl Write) -> Result<bool> {
    let work = root.join("target").join("pawnio");
    fs::create_dir_all(&work).with_context(|| format!("create {}", work.display()))?;

    fetch_zip(&work, out)?;
    let extracted = work.join(VERSION);
    fs::create_dir_all(&extracted).with_context(|| format!("create {}", extracted.display()))?;
    // tar runs inside `work` with relative names: an absolute `C:` path reads as a remote host to GNU tar.
    let tar = if Path::new(WINDOWS_TAR).exists() {
        WINDOWS_TAR
    } else {
        "tar"
    };
    run_tool(
        Command::new(tar)
            .current_dir(&work)
            .args(["-xf", ZIP_NAME, "-C", VERSION]),
    )?;

    for (name, expected) in FILES {
        let actual = sha256(&extracted.join(name))?;
        check_hash(name, expected, &actual)?;
    }

    for profile in ["debug", "release"] {
        let dir = root.join("target").join(profile).join("modules");
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        for (name, _) in FILES {
            fs::copy(extracted.join(name), dir.join(name))
                .with_context(|| format!("copy {name} to {}", dir.display()))?;
        }
        writeln!(
            out,
            "modules {VERSION}: {} files verified into {}",
            FILES.len(),
            dir.display()
        )?;
    }
    Ok(true)
}

fn fetch_zip(work: &Path, out: &mut impl Write) -> Result<()> {
    let zip = work.join(ZIP_NAME);
    if zip.exists() && sha256(&zip).is_ok_and(|actual| actual.eq_ignore_ascii_case(ZIP_SHA256)) {
        writeln!(out, "using the downloaded {ZIP_NAME}")?;
        return Ok(());
    }
    writeln!(out, "downloading {URL}")?;
    run_tool(
        Command::new("curl.exe")
            .current_dir(work)
            .args(["-L", "--fail", "-o", ZIP_NAME, URL]),
    )?;
    check_hash(ZIP_NAME, ZIP_SHA256, &sha256(&zip)?)
}

fn run_tool(command: &mut Command) -> Result<()> {
    let shown = format!("{command:?}");
    let status = command.status().with_context(|| format!("start {shown}"))?;
    ensure!(status.success(), "{shown} failed with {status}");
    Ok(())
}

fn sha256(path: &Path) -> Result<String> {
    let output = Command::new("certutil")
        .arg("-hashfile")
        .arg(path)
        .arg("SHA256")
        .output()
        .context("start certutil")?;
    let text = String::from_utf8_lossy(&output.stdout);
    parse_certutil_sha256(&text).with_context(|| format!("no SHA-256 in certutil's output for {}", path.display()))
}

/// The hash is line two of `certutil -hashfile`'s output, which may group the digits with spaces.
pub(crate) fn parse_certutil_sha256(output: &str) -> Option<String> {
    let hash: String = output.lines().nth(1)?.split_whitespace().collect();
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
}

pub(crate) fn check_hash(file: &str, expected: &str, actual: &str) -> Result<()> {
    if expected.eq_ignore_ascii_case(actual) {
        return Ok(());
    }
    bail!("{file} has the wrong SHA-256: expected {expected}, got {actual}")
}

#[cfg(test)]
mod tests;
