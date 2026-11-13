# 0019 - Extra readings come only from calls already made

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The owner asked: "give all possible options that can easily be got from the same source, no additional query loads". The overlay already queries the GPU performance data, the GPU engine counters, the memory status, every DDR5 hub, and the graphics kernel's present events once per update; each returns more than the four lines use.

The spec lists "readings beyond the four lines" as a non-goal. This decision departs from it for the values below only.

## Decision

1. **The rule: no new query.** An extra value may only keep more of what a call the app already makes returns: another field of the same struct, another instance of the same counter, another event of the same provider, another register of a hub already read. A new Win32 or D3DKMT call, counter path, PawnIO read, ETW provider, keyword, or event id per update is refused and goes to `docs/deferred.md`. A constant read once at start (the trace clock's frequency, which `OpenTraceW` writes into the log file header it was given) is not a query per update.
2. **The values, their sources, and their defaults**:
   - FPS line: FPS (on); frame time, `1000 / fps` (off); worst frame (off), the longest gap between two consecutive presents of the foreground process since the last reading, from the timestamp of the `Present_Info` events already counted. A gap over one second is a pause, not a frame, and is not counted. The last present's time carries over a reading, so a gap across two readings counts.
   - CPU line: load (on) and temperature (on), as before.
   - GPU line: load (on) and temperature (on), as before; fan in rpm (off), VRAM clock in MHz (off), both from the `KMTQAITYPE_ADAPTERPERFDATA` query that gives the temperature; video load (off), the busiest engine type whose name contains `video` and not `jpeg`, summed over processes, from the engine counters that give the load.
   - Memory line: load (on) and temperature (on), as before; used in GB (off), total minus available from the memory status that gives the load, in gibibytes as Task Manager says GB; each stick (off), the last value of each SPD5118 hub in address order, from the hubs already read for the hottest.
3. **Zero rules.** A driver leaves a field at 0 when it does not report it, and a zero-rpm fan also reports 0 at rest, so the fan is `—` until it has turned once in this run, after which every value, 0 included, is real. A VRAM clock of 0 is `—`. A GPU without a video engine, a frame rate with fewer than two presents, and a missing hub scan show `—`.
4. **Fixed widths.** Each value has a fixed width, right-aligned, and `—` takes the same width, so a column never jumps. The formats are in the plan and in the code that renders them.
5. **Left out**, because each would need another query or says too little:
   - `Power`: a share of an unstated limit in tenths of a percent, and 0 on the owner's card.
   - `MemoryBandwidth` and `PCIEBandwidth`: 0 and a constant 3 on the owner's card, with a driver-defined meaning.
   - `MaxMemoryFrequency`, `MaxMemoryFrequencyOC`, and the VRAM size: constants.
   - The CPU kernel and user split: noise next to the load.
   - Commit charge: page-file backed, rarely what a game feels.
   - Copy and compute engines: rarely busy in a game, and their names differ by vendor.
   - Intel TjMax and the AMD range bit: constants.
   - What needs its own query is in `docs/deferred.md`.

## Consequences

- Positive: the settings offer every value the app can get at no extra cost, and the cost of an update stays what it was.
- Negative: the sensor ports carry an `Extras` value beside the main reading, and the frame counter keeps a timestamp per present, a little more work on the trace thread.
- Neutral: the FPS timestamp is the event header's `TimeStamp`, in performance counter ticks because the session sets `ClientContext = 1`, converted with the frequency of the open trace.

## Alternatives considered

- **Query what is missing** (VRAM used, per-core load, core clocks): each costs a call per update and needs its own decision.
- **Show a fan of 0 at once**: on a card whose driver never reports the fan this reads as a stopped fan, which is a claim the app cannot back.
