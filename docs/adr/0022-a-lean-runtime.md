# 0022 - A lean runtime

- **Status**: accepted
- **Date**: 2026-11-13

## Context

Before the first public release the owner asked: "confirm is this using 45mb of memory during run / confirm if that is most optimal memory usage / also how much is the size of running file", chose to optimise first, and added: "it naming in task manager is rust-analyzer, change that to Overlay Monitor just like the app" and "give icon in task manager also".

Measured on the owner's running release build (commit 44cd02b, elevated, Settings closed): working set 37.6 MB, private working set (Task Manager's "Memory") 13.1 MB, private bytes 15.2 MB, 14 threads, 271 handles, 49 modules, exe 2,422,784 bytes with no version info. Among the largest images were `d2d1.dll` (6.5 MB), `D3D10Warp.dll` (5.7 MB), `d3d11.dll` (2.4 MB), and `WindowsCodecs.dll` (2.2 MB). The overlay stroked and filled its few lines of text with a Direct2D DC render target, which brings in Direct3D 11 and the WARP software rasterizer, with its worker threads, to draw on the CPU.

## Decision

1. **The exe says what it is.** `services/monitor/api/build.rs` writes the resource script: the icon and a `VERSIONINFO` with "Overlay Monitor" as the file description and product name, the version from `CARGO_PKG_VERSION` (typed once), and the copyright of `LICENSE`. Every top-level window carries the app icon (the window classes, and `WM_SETICON` on the settings dialog).
2. **A smaller release build.** The root `Cargo.toml` has `[profile.release]` with `lto = "fat"`, `codegen-units = 1`, and `strip = "symbols"`. The panic strategy stays `unwind`, so a scoped thread's panic still reaches `join`.
3. **Trim once, never periodically.** The working set is trimmed (`SetProcessWorkingSetSize(.., MAX, MAX)`) once about 5 seconds after start and once each time Settings closes, which first drops everything the window allocated.
4. **The overlay's text is drawn with DirectWrite alone.** `platform/win32/src/text.rs` places the glyphs as before (the same metrics, advances, and whole-pixel baselines, so the window has the same size for the same text and style), and `IDWriteFactory2::CreateGlyphRunAnalysis` rasterizes each line in grayscale without hinting (`NATURAL_SYMMETRIC`, grid fit off). DirectWrite's texture has 16 levels and puts glyphs on a quarter-pixel grid, so a line is rasterized up to 4 times finer than the canvas (at most 256 fine pixels per em) and averaged back. `platform/win32/src/coverage.rs` grows that coverage by the outline width into the outline's coverage, corners round: a partly covered pixel's value is read as how far the shape reaches past its centre, and growth reaching a pixel from opposite sides (a gap narrower than the outline) adds up. Background, outline, then glyphs are blended as premultiplied BGRA straight into the layered window's DIB, which `UpdateLayeredWindow` presents as before (ADR-0016). The unlocked overlay's alpha of at least 1 holds, because blending over the background never lowers its alpha. The `Win32_Graphics_Direct2D`, `Win32_Graphics_Direct2D_Common`, and `Win32_Graphics_Dxgi_Common` features go (the last stays for the test-only swap chain).

## Measurements

The owner's copy was not running during Part B, so a non-elevated release build of each stage was launched for about 10 seconds with the owner's settings (1 px outline), read, and closed. "Before trim" is 3 s after start, "after" is 10 s, past the start-up trim. The repository's measuring test (`reports_memory_after_a_frame_and_after_settings_closes`, release) draws the four default lines in its own process; it carries no icon, so it never loads `WindowsCodecs.dll`. Ranges are over three runs; MB is MiB.

| Measure | Before (44cd02b) | After 1-3 | After 4 |
| --- | --- | --- | --- |
| App working set before trim / after (MB) | 37.6 (owner's, long-running) | 28.7-28.8 / 6.8-8.1 | 20.8-20.9 / 2.8-3.2 |
| App private working set before trim / after (MB) | 13.1 | 10.6-10.7 / 3.0-3.8 | 7.8-7.9 / 1.1-1.2 |
| App private bytes (MB) | 15.2 | 12.1-13.1 | 8.5-9.1 |
| App threads / handles / modules | 14 / 271 / 49 | 15-17 / 229-235 / 42-43 | 6 / 184-190 / 33-34 |
| `d2d1` / `d3d11` / `D3D10Warp` / `WindowsCodecs` in the app | loaded | loaded | no / no / no / loaded |
| Test: working set after a frame (MB) | 19.2-19.6 | 19.0-19.1 | 10.6-11.4 |
| Test: private bytes after a frame (MB) | 5.2-5.6 | 5.1-5.2 | 2.0-2.7 |
| Test: working set with Settings open / after it closes (MB) | 24.7-25.1 / 24.8-25.1 | 24.7 / 1.4 | 16.6-17.3 / 1.3 |
| exe size (bytes) | 2,422,784 | 2,338,816 | 2,349,568 |

The new text against Direct2D's, pixel by pixel, for the four default lines at 125 % (286 x 126 pixels, identical sizes): the mean channel difference is 2.6 of 255; 98 pixels differ by more than 64, at sharp outer corners of the outline, where coverage alone cannot say where a corner lies inside its pixel. The glyph fill alone differs by at most 54. A redraw takes about 2 ms instead of 1.1 ms for the default style, and 20 ms instead of 2.5 ms for 72 pt with an 8 px outline at 125 %, at most once per update.

## Consequences

- Positive: Direct2D, Direct3D 11, and WARP are no longer loaded, and with them go about 10 threads, 8 MB of working set, and 3.5 MB of private bytes. The text still draws on the CPU, leaving the GPU to the game.
- Negative: the text is not bit-identical to before (above). A redraw costs more CPU, most at large sizes with a wide outline. The exe grows by 10.7 KB over step 3's, as the drawing code is now ours.
- Neutral: `WindowsCodecs.dll` still loads, but not for drawing: every frame of the app icon is PNG-compressed, and Windows decodes PNG icon frames through WIC when the tray and window icons are made. Storing the small frames uncompressed would keep WIC out at the cost of a larger exe; that is the owner's call.

## Alternatives considered

- **Keep Direct2D**: the simplest code, but it is the largest part of the memory and threads, for a few lines of text.
- **DirectWrite at canvas resolution, no oversampling**: 16 levels and quarter-pixel glyph placement made edges visibly coarser than before (mean difference 5.1, 319 pixels off by more than 64).
- **Outline the glyphs ourselves** (`GetGlyphRunOutline` into our own geometry sink, then our own stroker and rasterizer): exact strokes, but a COM sink and a vector rasterizer are far more code than growing coverage.
- **GDI paths** (`BeginPath`, `TextOutW`, `StrokeAndFillPath`): no anti-aliasing, and GDI leaves alpha at 0, so the layered window would be see-through where it draws.
