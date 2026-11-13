//! Embeds the exe's manifest and its file icon. The manifest asks for version 6 of the common controls, for themed
//! menus, and the elevation the build wants; feature `elevated` makes Windows ask for admin at start, which the
//! `PawnIO` temperatures need. The icon comes from `app.rc`, which holds no manifest, so the two do not collide.

use embed_resource::CompilationResult;

// clippy's print_stdout leaves build scripts alone, so only the environment read needs an expectation.
fn main() -> Result<(), CompilationResult> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
         version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    #[expect(
        clippy::disallowed_methods,
        reason = "cargo talks to a build script through stdout and its environment"
    )]
    let elevated = std::env::var_os("CARGO_FEATURE_ELEVATED").is_some();
    let level = if elevated { "requireAdministrator" } else { "asInvoker" };
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='{level}' uiAccess='false'");
    embed_resource::compile_for("app.rc", ["overlay-monitor"], embed_resource::NONE).manifest_required()
}
