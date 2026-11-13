---
name: writing-adrs
description: Use when a change needs a recorded decision (a new dependency, the toolchain or a pinned tool moving, a gate added or loosened, a published contract, or a choice the user made that a future change could contradict), or when asked to write, review, or supersede an ADR.
---

# Write an architecture decision record

A record exists so the next person or agent neither reopens a settled question nor quietly undoes it. Write it before the change that depends on it.

## Steps

1. **State the question in one sentence**: "This record decides X." Done when the sentence names one decision.
2. **List the real options**, at least two, and what separates them.
3. **When the choice is the user's, confirm before drafting**: give the question, the options, and your recommendation, and wait.
4. **Write the file** `docs/adr/NNNN-<kebab-title>.md` with the next number, in the shape of the existing records: Status, Date, then Context, Decision (numbered sentences a tool or reviewer can check), Consequences (positive, negative, neutral), and Alternatives considered (why each lost). Add its row to `docs/adr/README.md`.
5. **Status** stays `proposed` until the user agrees, then `accepted` in the file and the index.
6. **Done** when the record reads complete to someone who was not in the conversation, and the index row matches the file.

## Writing it well

- Put each rule in a checkable form: "every pool is opened by `platform-postgres`", not "use good connection practices".
- Name the cost honestly; a record with no negative consequence has not been thought through.
- Link what it depends on or replaces by number (`ADR-0009`).
- One decision per record.

## After acceptance

An accepted record's reasoning stays as written. To change a decision, write a record that supersedes it and set the old one's status (and index row) to `superseded by NNNN`. When a tool starts enforcing a decision, say so in `AGENTS.md`.
