# snipper

> A tiny native screen capture and markup tool for Windows: snip, mark up, Ctrl+C, paste.

## What it is

Snipper is a ~300 KB single-file Rust executable for Windows 10/11. It takes a screenshot of a
dragged rectangle, a window, or the full screen, optionally after a 3/5/10 second countdown. It
then opens the snip in a small editor where you can add outline rectangles, arrows, text,
highlighter boxes and freehand pen strokes, and crop. **Ctrl+C** puts the flattened image on the
clipboard (as a DIB and as PNG), so it pastes into Warmstart, Office, Slack, browsers and so on.
**Ctrl+S** saves a PNG.

It is not a screen recorder, has no cloud upload, and has no scrolling capture.

The UI is plain Win32 drawn with Direct2D/DirectWrite. It follows the system light or dark
theme, is per-monitor DPI aware, and works across multiple monitors. The only dependency is the
`windows` crate.

## Getting started

Requires the Rust MSVC toolchain (`rustup default stable-x86_64-pc-windows-msvc`) and the Visual
Studio C++ build tools.

```
cargo build --release
target\release\snipper.exe            # launcher window + tray icon
target\release\snipper.exe --region   # start a rectangle snip immediately (also --window, --full)
target\release\snipper.exe --tray     # start hidden in the tray (e.g. from shell:startup)
```

Only one instance runs at a time. Launching again forwards the request to the running one.

### Using it

- **Launcher:** **New** starts a snip. The two dropdowns pick the mode (Rectangle / Window /
  Full screen) and the delay. **⋯** picks the global shortcut (default **Ctrl+Print Screen**) or
  exits. Closing the launcher keeps Snipper in the tray.
- **Overlay:** drag to select, or in window mode click a highlighted window. The bar at the top
  switches mode (keys R / W / F). Esc or right-click cancels.
- **Editor:** the capture is copied to the clipboard as soon as it opens.

| Key | Action |
| --- | --- |
| V / R / A / T / H / P / C | Select, Rectangle, Arrow, Text, Highlighter, Pen, Crop |
| Ctrl+C / Ctrl+S | Copy flattened image / save PNG |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Del, arrow keys | Delete / nudge the selected shape (Shift = 10 px) |
| Shift while drawing | Square rectangles, 45° arrows |
| Ctrl+wheel, Ctrl+0, Ctrl+1 | Zoom, fit, 100% |
| Space+drag or middle-drag | Pan |
| Ctrl+N / Ctrl+W | New snip / close |

Shapes stay editable. Select one to move it, drag its handles, or recolour or resize it from the
toolbar. Double-click text to edit it. Text entry supports IME input (e.g. Korean). Settings
(mode, delay, shortcut, per-tool colour and size, last save folder) live in
`%APPDATA%\Snipper\settings.ini`.

## Checks

Run these before committing:

- `cargo test`
- `cargo clippy`
- `cargo build --release`

## Layout

- `src/main.rs`: entry point, command-line flags, single instance, message loop.
- `src/app.rs`: capture pipeline (hide → countdown → freeze → overlay → editor) and the hotkey.
- `src/launcher.rs`: launcher window, tray icon and menus; hosts timers and app messages.
- `src/capture.rs`: GDI grab of the virtual screen plus visible window rectangles.
- `src/overlay.rs`: full-screen selection overlay (region/window modes, loupe, mode bar).
- `src/countdown.rs`: the delay countdown tile.
- `src/editor.rs`: markup editor window (toolbar, canvas, tools, text entry, IME).
- `src/doc.rs`: shapes, hit testing, handles, undo/redo, flattening.
- `src/gfx.rs`: Direct2D/DirectWrite/WIC helpers, off-screen render, PNG encode.
- `src/ui.rs`: window plumbing, theme, vector icons, buttons, menus, runtime app icon.
- `src/output.rs`: clipboard (CF_DIB + PNG), save dialog, PNG files.
- `src/settings.rs`: persisted preferences.
- `build.rs`, `snipper.manifest`: embed the DPI/common-controls manifest via the MSVC linker.

---

Companion docs: [`AGENTS.md`](AGENTS.md) for how coding agents work here, and
[`HANDOFF.md`](HANDOFF.md) for the current state and what to pick up next.
