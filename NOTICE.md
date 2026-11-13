# Third-party material

Only MIT, BSD, and Apache-2.0 material is brought in. For each item keep the upstream license text in the repository, record the upstream and version below, and prefer a dependency to a copy. A new dependency needs an ADR.

## Vendored or adapted

None yet. Configuration was written fresh after reading the projects in the README Credits.

## Crates (used as dependencies, not copied)

Their versions are pinned once in the root `Cargo.toml` and their licenses are checked by `cargo deny check licenses` against the allowlist in `deny.toml`. `anyhow`, `clap`, `syn`, `cargo_metadata`, `toml_edit`, `walkdir`, and `regex` belong to `xtask` only.

## Shipped in the release zip, not in this repository

The release zip's `modules` folder carries four binary modules from [PawnIO.Modules](https://github.com/namazso/PawnIO.Modules) 0.2.11 (`AMDFamily17.bin`, `IntelMSR.bin`, `SmbusPIIX4.bin`, `SmbusI801.bin`), unmodified, under the GNU Lesser General Public License 2.1. Their license text ships beside them as `COPYING`, and `SOURCE.txt` links their source at that version. They are downloaded and hash-checked by `cargo xtask modules` and never committed. The PawnIO driver itself is not shipped: users install it from its author.

## Developer tools (installed, not copied)

cargo-nextest, cargo-deny, cargo-shear, typos, lefthook, zizmor. Each keeps its own license; `cargo xtask doctor` checks their versions.
