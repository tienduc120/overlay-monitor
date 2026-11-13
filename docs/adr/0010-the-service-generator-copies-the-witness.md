# 0010 - The service generator copies the witness

- **Status**: superseded by 0017
- **Date**: 2026-11-12

## Context

An agent starting a service from a blank directory makes the same first mistakes each time. go-base kept templates and a generated witness and diffed them. A template is a second copy that can drift.

## Decision

1. **`services/example` (aggregate `Note`) is the witness**: a complete service, one crate per layer, every file with logic tested, passing every gate as part of the workspace.
2. **`cargo xtask new-service <name> --aggregate <Name>` copies it**, renaming `example` and `Note` in paths and contents. It refuses a name that exists, a name that is not kebab-case, or an aggregate that is not PascalCase.
3. **There are no templates.** The witness is the template, so the generator cannot drift from code that compiles and passes the gates.

## Consequences

- Positive: a new service starts green; changing the witness changes every future service, and the gates check the change.
- Negative: the witness's words (`example`, `Note`, `note`) must not appear in it with another meaning, or the rename hits them. Plurals are the singular plus "s".
- Neutral: the witness is a real service that does nothing useful.

## Alternatives considered

- **Templates plus a drift test**: two copies kept in step by a test.
- **cargo-generate**: a second tool, and the templates could not be compiled and linted as code.
