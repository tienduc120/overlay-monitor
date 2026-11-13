# 0012 - Unsafe code lives only in platform-win32

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The base forbids `unsafe` everywhere (ADR-0004). Overlay Monitor must call Win32 and ETW, which are `unsafe` in the `windows` crate. A `forbid` cannot be lifted even for one crate, so the app needs a narrower rule that still keeps `unsafe` in one reviewable place. Approved by the owner on 2026-11-13.

## Decision

1. **`unsafe_code` is `deny` in `[workspace.lints.rust]`**, so it is refused everywhere unless a crate expects it.
2. **Only `platform-win32` expects it**, once, crate-wide: `#![expect(unsafe_code, reason = "...")]` in its `lib.rs`. This is the sanctioned form; a per-item `#[expect]` is not needed there.
3. **`source-unsafe-scope`** (`cargo xtask verify`, fixture `source-unsafe-scope`) refuses an `expect` or `allow` of `unsafe_code`, inner or outer, in any other crate.
4. **`clippy::undocumented_unsafe_blocks` is `deny`**: every `unsafe` block carries a `// SAFETY:` comment.
5. **Every `unsafe` call is wrapped in a safe function** of `platform-win32`; no other crate sees an unsafe signature.

## Consequences

- Positive: the whole `unsafe` surface is one crate a reviewer can read; a mistake elsewhere does not compile.
- Negative: the crate-wide expectation hides which functions are unsafe except by reading the `// SAFETY:` comments; the crate must contain at least one `unsafe` block or `#![expect]` becomes an unfulfilled-expectation error.
- Neutral: the existing `unsafe-code` fixture keeps proving the lint fires in any other crate.

## Alternatives considered

- **Keep `forbid` and avoid Win32 FFI**: the app cannot read CPU, GPU, or frame data without it.
- **A per-item `#[expect]` in `platform-win32`**: noisy, and adds nothing the crate boundary does not already give.
- **Allow `unsafe` in any platform crate**: spreads the reviewable surface across crates.
