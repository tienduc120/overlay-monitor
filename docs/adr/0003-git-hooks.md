# 0003 - Git hooks

- **Status**: accepted
- **Date**: 2026-11-12

## Context

Gates that run only when someone remembers them are skipped by the author who most needs them. Git hooks are the one layer every tool and every human passes through, whether or not their tool supports agent hooks.

## Decision

1. **lefthook** manages the hooks (`lefthook.yml`): one binary, the same on Windows and Linux.
2. **pre-commit** runs `cargo xtask verify`.
3. **commit-msg** runs `cargo xtask commitmsg`: Conventional Commits, a lowercase imperative subject with no trailing period, at most 72 characters, a blank line before the body, every problem reported at once. A `refactor:` commit that changes or removes an existing test line is refused.
4. **pre-push** runs `cargo xtask verify --full`.
5. Hooks are installed per clone with `lefthook install`; `cargo xtask doctor` reports when they are not. Bypassing them is forbidden by `AGENTS.md` and refused by the agent guard.

## Consequences

- Positive: a failing gate stops the commit, with the same output an agent would see running `verify` by hand.
- Negative: every commit pays for clippy and the tests.
- Neutral: hooks are a guard rail; CI runs the same gates.

## Alternatives considered

- **cargo-husky or pre-commit (Python)**: one is unmaintained, the other a second runtime.
- **CI only**: feedback arrives after the push, and an agent loop wants it before the commit.
