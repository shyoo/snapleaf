<p align="center">
  <img src="assets/snapleaf-192.png" alt="Snapleaf leaf and capture frame icon" width="96" height="96">
</p>

# Snapleaf

> A tiny native screenshot and markup tool for Windows. Capture, annotate, copy.

## What it is

Snapleaf is a small, portable Rust executable for Windows 10/11. It takes a screenshot of a
dragged rectangle, a window, or the full screen, optionally after a 3/5/10 second countdown. It
then opens the snip in a small editor where you can add outline rectangles, arrows, text,
highlighter boxes and freehand pen strokes, and crop. **Ctrl+C** puts the flattened image on the
clipboard (as a DIB and as PNG), so it pastes into Warmstart, Office, Slack, browsers and so on.
**Ctrl+S** saves a PNG.

Captures stay on your computer. There is no cloud upload, screen recording, or scrolling capture.

The UI is plain Win32 drawn with Direct2D/DirectWrite. It follows the system light or dark
theme, is per-monitor DPI aware, and works across multiple monitors. It uses the `windows`
and `windows-numerics` crates and needs no separate runtime installation.

## Getting started

Run `snapleaf.exe` to open the launcher. Use **New** to capture, add your annotations,
then press **Ctrl+C** to copy or **Ctrl+S** to save a PNG. Closing the launcher keeps
Snapleaf available in the notification area; use **Exit Snapleaf** in its tray menu to quit.
Windows may initially place the tray icon in the overflow menu.

### Build from source

Requires the Rust MSVC toolchain (`rustup default stable-x86_64-pc-windows-msvc`) and the Visual
Studio C++ build tools.

To produce a release (runs the tests, then writes `dist\snapleaf.exe` and
`dist\snapleaf-<version>-x64.zip` with a `.sha256`):

```
powershell -ExecutionPolicy Bypass -File scripts\release.ps1   # -SkipTests, -StopRunning
```

Or build by hand:

```
cargo build --release
target\release\snapleaf.exe            # launcher window + tray icon
target\release\snapleaf.exe --region   # start a rectangle snip immediately (also --window, --full)
target\release\snapleaf.exe --tray     # start hidden in the tray (e.g. from shell:startup)
```

Only one instance runs at a time. Launching again forwards the request to the running one.

### Using it

- **Launcher:** **New** starts a snip. The two dropdowns pick the mode (Rectangle / Window /
  Full screen) and the delay. **⋯** picks the global shortcut (default **Ctrl+Print Screen**) or
  exits. Closing the launcher keeps Snapleaf in the tray.
- **Overlay:** drag to select, or in window mode click a highlighted window. The bar at the top
  switches mode (keys R / W / F). Esc or right-click cancels.
- Snapleaf hides its launcher and open editors during capture so they do not cover the subject.
  Existing editors return afterward; cancelling also brings back the launcher if it was open.
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
| Ctrl+B / Ctrl+I | Bold / italic for the selected or next text |
| Ctrl+N / Ctrl+W | New snip / close |

Shapes stay editable. Select one to move it, drag its handles, or recolour or resize it from the
toolbar. Double-click text to edit it. Text entry supports IME input (e.g. Korean). The font
button picks the typeface for the selected text, or for new text while the Text tool is active.
It offers common Windows fonts, Bold, Italic, and **More fonts…** (the system font dialog). You
can change the font mid-typing without ending the edit. Settings (mode, delay, shortcut, per-tool
colour and size, text font, last save folder) live in `%APPDATA%\Snapleaf\settings.ini`.
If that file does not exist, Snapleaf reads `%APPDATA%\Snipper\settings.ini` to carry forward
preferences from the previous name. Future changes save to the Snapleaf folder.

## Icon and favicon assets

The Snapleaf mark pairs a pale green leaf with white capture corners on a green tile.
The same icon appears in Explorer, the tray, and window title bars.

| Asset | Use |
| --- | --- |
| [`snapleaf.svg`](assets/snapleaf.svg) | Scalable logo |
| [`snapleaf.ico`](assets/snapleaf.ico) | Windows app icon, 16–256 px |
| [`favicon.ico`](assets/favicon.ico) | Web favicon, 16/32/48 px |
| [`favicon-16x16.png`](assets/favicon-16x16.png), [`favicon-32x32.png`](assets/favicon-32x32.png) | PNG favicons |
| [`apple-touch-icon.png`](assets/apple-touch-icon.png) | 180 px touch icon |
| [`snapleaf-192.png`](assets/snapleaf-192.png), [`snapleaf-512.png`](assets/snapleaf-512.png) | Larger app and project images |

For a website, copy the assets to its public directory and reference them in the page head:

```html
<link rel="icon" href="/assets/favicon.ico" sizes="any">
<link rel="icon" href="/assets/snapleaf.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/assets/apple-touch-icon.png">
```

Recreate the PNG and ICO files with Windows PowerShell 5.1:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\new-app-icon.ps1
```

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
- `src/fonts.rs`: font menu choices and the system font dialog, mapped to DirectWrite families.
- `src/gfx.rs`: Direct2D/DirectWrite/WIC helpers, off-screen render, PNG encode.
- `src/ui.rs`: window plumbing, theme, vector icons, buttons, menus, DPI-sized app icon loading.
- `src/output.rs`: clipboard (CF_DIB + PNG), save dialog, PNG files.
- `src/settings.rs`: persisted preferences.
- `scripts/release.ps1`: test, build and package into `dist/` (gitignored).
- `build.rs`, `snapleaf.manifest`, `assets/snapleaf.ico`: embed the manifest and multi-size EXE icon
  via the MSVC linker, without a resource compiler. `scripts/new-app-icon.ps1` recreates the icon
  artwork used by Explorer (run with Windows PowerShell 5.1).

---

Companion docs: [`AGENTS.md`](AGENTS.md) for how coding agents work here, and
[`HANDOFF.md`](HANDOFF.md) for the current state and what to pick up next.
