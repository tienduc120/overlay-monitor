//! Embeds the exe's manifest, its file icon, and its version info. The manifest asks for version 6 of the common
//! controls, for themed menus, and the elevation the build wants; feature `elevated` makes Windows ask for admin at
//! start, which the `PawnIO` temperatures need. The icon and the version info come from a resource script written
//! here from the package's version, which holds no manifest, so the two do not collide.

use std::path::PathBuf;

#[path = "version_info.rs"]
mod version_info;

// clippy's print_stdout leaves build scripts alone, so only the environment reads need an expectation.
#[expect(
    clippy::disallowed_methods,
    reason = "cargo talks to a build script through stdout and its environment"
)]
fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=version_info.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
         version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    let elevated = std::env::var_os("CARGO_FEATURE_ELEVATED").is_some();
    let level = if elevated { "requireAdministrator" } else { "asInvoker" };
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='{level}' uiAccess='false'");

    let version = std::env::var("CARGO_PKG_VERSION").map_err(|error| format!("CARGO_PKG_VERSION: {error}"))?;
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?;
    let out_dir = std::env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?;
    // The resource compiler reads forward slashes in a path inside a string.
    let icon = PathBuf::from(manifest_dir).join("assets").join("icon.ico");
    let script = version_info::resource_script(&version, &icon.to_string_lossy().replace('\\', "/"));
    let script_path = PathBuf::from(out_dir).join("app.rc");
    std::fs::write(&script_path, script).map_err(|error| format!("write the resource script: {error}"))?;
    embed_resource::compile_for(script_path, ["overlay-monitor"], embed_resource::NONE)
        .manifest_required()
        .map_err(|result| format!("embed the resources: {result:?}"))
}
