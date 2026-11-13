# 0018 - The settings file

- **Status**: accepted
- **Date**: 2026-11-13

## Context

The overlay's look, lines, shortcuts, and place must survive a restart, and the owner wants to edit them by hand as well as through menus. The file is a contract with the user: its key names are what they type.

## Decision

1. **The path** is `%APPDATA%\OverlayMonitor\settings.toml`, found with `SHGetKnownFolderPath(FOLDERID_RoamingAppData)` in `platform-win32`, never from the environment. The log folder is `%LOCALAPPDATA%\OverlayMonitor\logs` (`FOLDERID_LocalAppData`).
2. **The format is TOML**, and every key is optional. A key left out takes its default, so a file with only `locked = true` is complete. A line's value key that the line does not have (`fan` under `[lines.cpu]`) is ignored.
3. **The keys are the contract.** Top level: `update_seconds`, `show_hide`, `lock_unlock`, `start_with_windows`, `visible`, `locked`, and `gpu` (written only when one is chosen; absent means the most dedicated video memory). `[style]`: `font`, `size_pt`, `bold`, `italic`, `text`, `outline`, `outline_px`, `background`, `background_opacity_percent`. `[lines.fps]`, `[lines.cpu]`, `[lines.gpu]`, `[lines.memory]`, each with `enabled`, `label`, and its values: fps `fps`, `frame_time`, `worst_frame_time`; cpu `load`, `temperature`; gpu `load`, `temperature`, `fan`, `vram_clock`, `video`; memory `load`, `used`, `temperature`, `sticks`. `[position]` (written only once the overlay has been placed): `monitor`, `x`, `y`. Colours are `"#RRGGBB"` and shortcuts are text such as `"Ctrl+Shift+O"`. The first line is a comment telling the user to edit the file while the app is closed; every save writes it.
4. **A wrong type is an error; a wrong value is not.** A value of the wrong TOML type (`size_pt = "big"`), or text that is not TOML, makes the file unreadable. A string that is not a colour or a usable shortcut takes that key's default, and a number out of range is clamped, so `background_opacity_percent = 300` or a huge `x` is never a type error. Whole and fractional numbers are both accepted wherever a number is.
5. **An unreadable file is kept.** It is copied to `settings.toml.bad` (replacing an older copy) and the load reports `Unreadable` with that name, so the next save does not destroy what the user wrote. The app then runs on the defaults.
6. **A save is atomic.** It creates the folder, writes `settings.toml.tmp`, and renames it over `settings.toml`, so a crash mid-write never leaves an unreadable file.
7. **The position is a monitor device name plus an offset** inside that monitor (`\\.\DISPLAY1`, `x`, `y`), not absolute desktop coordinates, so it survives monitors being rearranged.
8. **`serde` and `toml` are used by `monitor-infrastructure` only.** The domain is `#![no_std]` and the application's allow list is `thiserror`, `tracing`, `jiff`, so neither can hold the derives. The file's shape lives in private structs in infrastructure, which convert to and from the application's `Settings`.

## Consequences

- Positive: a hand-edited file rarely breaks the app, and when it does the user's text is kept. The application stays free of the file format.
- Negative: the private structs repeat the settings' shape, and the key names are a published contract that a later change must keep or migrate.
- Neutral: `toml` moves from the tooling group to the runtime group of the workspace dependencies.

## Alternatives considered

- **JSON**: no comments, so the file cannot carry its own advice, and it is harder to edit by hand.
- **The registry**: not portable, not hand-editable in a text editor, and not visible to the user as a file.
- **`serde` derives on the application's types**: would put a format dependency into the application and break its allow list, and a renamed field would change the file silently.
