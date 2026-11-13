# 0016 - Overlay presentation

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The overlay sits on top of borderless games. An extra window over a game can stop Windows from handing the game's frames straight to the display (independent flip). The game is then composed by the desktop window manager, and FreeSync stops following its frame rate. The spec's anti-goal is silently costing a game FreeSync or a frame of latency. Two ways to present the overlay were built as a spike: a layered window (`UpdateLayeredWindow`) and a composition swap chain (DirectComposition). The owner measured both with PresentMon 2.6.0 on 2026-11-13.

Setup: Path of Exile (`PathOfExile.exe`, DXGI, sync interval 0, no tearing flag), borderless, on the main monitor (2560x1440 at 240 Hz, Radeon RX 9060 XT, FreeSync on in AMD Software). Each run is 20 seconds of normal play in the same kind of scene.

| Run | Presents | Present mode | Frame rate | Display intervals |
| --- | --- | --- | --- | --- |
| No overlay | 2,208 | 100 % `Composed: Flip` | 111 fps | on the 240 Hz grid (8.3 ms, 4.2 ms, 12.5 ms) |
| Layered overlay | 2,391 | 100 % `Composed: Flip` | 120 fps | on the 240 Hz grid |
| Composition overlay | 2,429 | 100 % `Composed: Flip` | 122 fps | on the 240 Hz grid |

The plan's pass mark was 95 % `Hardware: Independent Flip` or `Hardware Composed: Independent Flip` while the overlay is up. No run passed, including the run with no overlay: on this PC the game was composed, and FreeSync was not following it, before any overlay appeared. Neither overlay changed the present mode or the display timing; the frame-rate spread is scene noise.

## Decision

1. **The overlay stays a layered window.** `platform/win32/src/overlay.rs` keeps `UpdateLayeredWindow`; the composition spike is not ported.
2. **The owner accepts the result as "no measurable cost on this PC"** (2026-11-13). The plan's fallback rule (build a move-to-the-other-monitor fallback when neither presentation passes) does not apply, because its premise, a game that reaches independent flip without the overlay, was not met.
3. **The fallback is not built.** It moves to `docs/deferred.md`, triggered by a measurement that shows a cost.

## Consequences

- Positive: no new presentation code, no new dependency, and nothing changes for the owner.
- Negative: the question this record was meant to settle, whether the layered overlay knocks a game out of independent flip, is still unmeasured, because the test game never reached independent flip. A PC or game that does reach it may lose FreeSync while the overlay is shown.
- Neutral: the owner's borderless games appear to be composed with or without the overlay (all three runs), which is a setting on the PC (for example "Optimizations for windowed games", or another window on top), not something this app causes.

## Alternatives considered

- **Composition swap chain**: measured the same as the layered window here, and it is more code (DirectComposition device, swap chain, and resize handling) for no measured gain.
- **Build the move-to-the-other-monitor fallback now**: it solves a cost nobody has measured, and it changes where the overlay appears.
- **Find why the game is composed and measure again**: the honest test, but the owner chose to stop at this result; it is the trigger in `docs/deferred.md`.
