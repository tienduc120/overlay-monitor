---
name: reviewing-code
description: Use when reviewing a change or a pull request, and before reporting work as finished. A checklist of what the gates cannot see.
---

# Review a change

The gates catch what a tool can name: layer edges, lints, manifests, aggregate shape, test timeouts. A review is for the rest. Read the gate output first, then the change in this order, and apply every rung.

1. **Right change.** It does what was asked and nothing more; drive-by renames and speculative options are separate changes. It was written test first: the tests fail without the code. A decision a future change could contradict has an ADR (skill: `writing-adrs`). Every gate is as strict as before, and any new `#[expect]` carries a reason the user has seen.
2. **Names and shape.** A name says what the thing is for in the domain's words. A function does one thing at one level. A comment says why. A new `pub` item or dependency is an architectural change and is justified as one.
3. **Layers.** Business rules live in the domain, use cases only orchestrate, adapters only translate, interfaces only map. A rule found in a handler or an adapter is in the wrong place. Time and ids arrive as values or through ports.
4. **Failure.** Every error keeps its cause and is translated once at its boundary; only interfaces picks a status; nothing internal reaches a client (skill: `handling-errors`).
5. **Effects and concurrency.** An effect that cannot be rolled back sits outside the writes that must agree; a retry cannot repeat it. Concurrent work goes through `platform-task`, every wait has a bound, and a lock guard is released before an `.await`.
6. **Evidence.** The author names the commands they ran and what they printed, and what they skipped and why.

## Report

Lead with whether it should merge. Then the findings, worst first, each with the file and line, what is wrong, and what to do. Mark what you did not read. A clean review is short.
