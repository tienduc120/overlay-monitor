# 0015 - CI runs on Windows

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The base ran `verify` on `ubuntu-latest`. This app depends on `windows` and builds only on Windows, so a Linux job cannot compile or test it. Approved by the owner on 2026-11-13.

## Decision

1. **The `verify` job runs on `windows-latest`** and runs `cargo xtask verify --full` as before.
2. **That job sets `defaults: run: shell: bash`**, so the commit-message loop and the clean-worktree check run unchanged.
3. **The `workflows` job (zizmor) stays on `ubuntu-latest`**; it does not build the app.

## Consequences

- Positive: CI compiles and tests what ships.
- Negative: Windows runners are slower and cost more minutes.
- Neutral: the pinned tool versions and action SHAs are unchanged.

## Alternatives considered

- **Cross-check on Linux**: the app does not compile there.
- **Both operating systems**: doubles cost for no extra coverage of a Windows-only app.
