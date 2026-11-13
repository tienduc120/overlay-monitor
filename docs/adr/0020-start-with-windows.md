# 0020 - Start with Windows through a scheduled task

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The app asks for administrator rights (the PawnIO temperatures and the frame trace need them). Starting it at sign-in must not show a UAC prompt each time, and it must keep running for as long as the user is signed in.

## Decision

1. **A Task Scheduler task at logon with the highest privileges** starts the exe. A task whose principal has `RunLevel` `HighestAvailable` and `LogonType` `InteractiveToken` runs elevated without a prompt, because the user approved it once when it was registered. Its name is `Overlay Monitor`.
2. **It is registered with `schtasks.exe` and an XML definition**, not the Task Scheduler COM API. The definition is a pure function (`task_xml`) that a test reads, and no new `unsafe` is needed. The file is UTF-16 with a byte order mark and is deleted after the call. Removing the task first asks `/Query` and deletes only what is there.
3. **The XML, not `/SC ONLOGON`**, because a task made by the command line takes the scheduler's defaults: it is stopped after 72 hours and when the power goes to the battery. The definition sets `ExecutionTimeLimit` to `PT0S` and both battery flags to false, so the overlay runs as long as the user is signed in.
4. **The task is registered again at each start** when `start_with_windows` is on, so it follows an exe that was moved or rebuilt elsewhere. A failure is logged and the app runs on.
5. **Registering needs an elevated app.** Only the release build with the `elevated` feature (the manifest asks for administrator) can create the task; an unelevated copy logs the refusal. Tests that really register a task are `#[ignore = "needs an elevated terminal"]` and use their own task name, never the owner's.
6. **The user's SID**, from the process token (`OpenProcessToken`, `GetTokenInformation`, `ConvertSidToStringSidW`) in `platform-win32`, names the trigger's and the principal's user, so the task belongs to the account that registered it and not to whoever signs in.

## Consequences

- Positive: no prompt at sign-in, no time or battery limit, and the exe path follows the app.
- Negative: the app depends on `schtasks.exe` and its output format only for error text; a policy that forbids the task makes the setting fail with the tool's message.
- Neutral: the settings window's checkbox arrives later and calls the same port.

## Alternatives considered

- **The `Run` registry key**: starts the exe unelevated or shows a UAC prompt at every sign-in.
- **The COM `ITaskService`**: the same result, but a large `unsafe` surface and a definition that cannot be read in a test.
- **A Windows service**: runs outside the user's session, so it cannot draw an overlay on the desktop.
