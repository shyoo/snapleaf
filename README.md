<p align="center">
  <img src="assets/snapleaf-192.png" alt="Snapleaf" width="96" height="96">
</p>

# Snapleaf

A tiny screenshot and markup tool for Windows. **Capture, annotate, copy.**

Snapleaf captures a rectangle, a window, or your full screen, then opens it in an editor.
Add arrows, text, rectangles, highlights and freehand marks, or crop the image before sharing.
Captures stay on your computer.

![Snapleaf editor annotating a capture of its own logo and launcher](docs/screenshots/editor.png)

## Download

Get the Windows x64 **setup installer** or portable ZIP from [Releases](https://github.com/shyoo/snapleaf/releases).
The installer adds Start menu and optional desktop shortcuts, and installs for your account
without administrator access. For a portable copy, extract the ZIP and run **snapleaf.exe**.
Requires Windows 10 or 11; no separate runtime is needed.

Snapleaf checks for stable updates in the background and downloads the installer after verifying
its SHA-256 checksum. Open it from **⋯ → Open downloaded update installer** when ready. Save your
images and exit Snapleaf from the tray before installing. You can check manually or turn off
automatic downloads from **⋯** or the tray menu. Preview releases are never downloaded
automatically. Updating a portable copy through the installer creates an installed copy;
you can instead download and extract the new portable ZIP.

Development builds are available as `snapleaf-windows-x64` artifacts on successful
[Actions runs](https://github.com/shyoo/snapleaf/actions/workflows/ci.yml)
(sign in to GitHub to download them).

## Capture

Open Snapleaf and click **New**, or press **Ctrl+Print Screen**. Choose Rectangle, Window,
or Full screen, with an optional 3, 5, or 10 second delay. Drag to select a rectangle;
press **Esc** or right-click to cancel.

![Snapleaf launcher with capture mode and delay controls](docs/screenshots/launcher.png)

![Selecting a rectangle around a demo containing Snapleaf's own artwork](docs/screenshots/capture.png)

Snapleaf supports multiple monitors and different display scales. Its launcher and editors
hide during capture, then return afterward. Closing the launcher keeps it in the notification
area; use **Exit Snapleaf** in the tray menu to quit. Change the global shortcut from **⋯**.

## Annotate and share

The fresh capture is copied automatically. Add your markup, then press **Ctrl+C** to copy
the updated image or **Ctrl+S** to save a PNG. Paste into chat, email, documents, or a browser.

Shapes remain editable: select one to move, resize, recolour, or delete it. Double-click text
to edit it. Text supports font choices, bold, italic, and IME input. While editing text,
use arrow keys to move between characters and lines, Shift+arrows or mouse dragging to
select, and Ctrl+A to select all. Typing or pasting replaces selected text; Ctrl+C copies
selected text (or the image if nothing is selected), and Ctrl+Shift+C always copies the image.
Ctrl+X cuts selected text. Press Esc or Ctrl+Enter to finish editing.

| Shortcut | Action |
| --- | --- |
| V / R / A / T / H / P / C | Select / rectangle / arrow / text / highlight / pen / crop |
| Ctrl+C / Ctrl+S | Copy image / save PNG |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Delete / arrow keys | Delete / nudge selection (Shift for 10 pixels) |
| Shift while drawing | Square rectangles / 45° arrows |
| Ctrl+wheel / Ctrl+0 / Ctrl+1 | Zoom / fit / actual size |
| Space+drag / middle-drag | Pan |
| Ctrl+B / Ctrl+I | Bold / italic text |
| Ctrl+Left / Ctrl+Right | Move by word while editing text (Shift to select) |
| Home / End / Ctrl+Home / Ctrl+End | Line / text boundaries while editing (Shift to select) |
| Ctrl+Backspace / Ctrl+Delete | Delete preceding / following word while editing text |
| Ctrl+Shift+C | Copy image while editing text |
| Ctrl+N / Ctrl+W | New capture / close editor |

## Build from source

Install the Rust MSVC toolchain and Visual Studio C++ build tools, then run:

```powershell
cargo build --release --locked
.\target\release\snapleaf.exe
```

See [development and release instructions](docs/development.md) for packaging, automation,
and contributor details.

## License

Copyright 2026 Sunghwan Yoo. Licensed under the [Apache License 2.0](LICENSE).
