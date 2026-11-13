# 0021 - The log file

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The release exe has no console, so what `tracing` writes to stderr is lost, and a startup error ends the process with nothing on screen. A friend whose reading shows a dash, or whose copy will not start, has nowhere to look.

## Decision

1. **A rolling log file** in `%LOCALAPPDATA%\OverlayMonitor\logs`: `overlay-monitor.log`, then `overlay-monitor.1.log` (the newest old one) up to `overlay-monitor.3.log`. Before a write that would take the file past 1 MiB, each old file moves up one and the oldest is dropped. Opening appends to a log that is already there.
2. **Our own writer, `RollingFile`**, in `monitor-infrastructure`, used through `tracing-subscriber`'s `MakeWriter` for `Mutex<W>`. Not `tracing-appender`: it rotates by time only, and its non-blocking writer runs a detached thread, which ADR-0014 forbids. A log line is a short write to a local file, so writing inline under a mutex costs little.
3. **Two `fmt` layers** on the registry under one `EnvFilter`: stderr as before, and the file with colours off. A folder or file that cannot be opened leaves stderr alone and logs a warning.
4. **What is logged**: a sensor's outcome when it changes (a failure at warn with its error, an unavailable reading at info with its reason, a value after either at info as "reading recovered"), never every sample; the start-up findings (frame counting needs admin, the PawnIO state); and the failures to start or to save.
5. **A startup failure is shown.** Each failure that ends the process (the single-instance claim, opening the overlay) is logged at error and shown in a message box (`MessageBoxW`) with the error, its causes, and the log folder. The text and the decision to show it are tested; the box itself is a thin call that is not.

## Consequences

- Positive: a bug report can carry a file, and a failed start says why.
- Negative: about 4 MiB of disk at most; a write that fails (a full disk) loses the line.
- Neutral: the sampler's thread logs through the same subscriber, so no other thread is needed.

## Alternatives considered

- **`tracing-appender`**: time-based rotation only, and a detached writer thread.
- **Windows Event Log**: needs a registered source and `unsafe` surface for a few lines of text a user cannot easily send.
- **Logging every sample**: grows the file with nothing a change does not already say.
