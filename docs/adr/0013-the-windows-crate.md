# 0013 - The windows crate

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The app reads CPU times, GPU data, and ETW frame events through Win32. It needs bindings; hand-written `extern` blocks would multiply the `unsafe` surface (ADR-0012). Approved by the owner on 2026-11-13.

## Decision

1. **The `windows` crate, version 0.62.2** (MIT OR Apache-2.0, Microsoft, <https://github.com/microsoft/windows-rs>), is pinned once in the root `[workspace.dependencies]`.
2. **Features live in the platform crate that uses them**: a member writes `windows = { workspace = true, features = [...] }`, so the root stays feature-free.
3. **`architecture.toml` denies `windows` to `infrastructure`, `interfaces`, and `api`**; `domain` and `application` already exclude it through `allow`. Windows calls go through `platform-win32`.
4. **The README credits it** (AGENTS.md: a new dependency is credited).

## Consequences

- Positive: generated, maintained bindings replace hand-written FFI; the layer rules keep them out of services.
- Negative: a large dependency with long compile times; only Windows builds the app.
- Neutral: its licence is on the `deny.toml` allow list.

## Alternatives considered

- **`windows-sys`**: no safe wrappers or `Result` types, so more code in the unsafe crate.
- **Hand-written `extern` declarations**: more unsafe surface and no maintainer.
- **`winapi`**: unmaintained.
