# snipper — Handoff

**Current state and what to do next, not a changelog.** Replaced as work lands, never stacked.

## Where things stand

v1 of the capture + markup tool is built (Rust, Win32 + Direct2D, `windows` 0.62). See README
for features and layout.

- Capture: rectangle, window and full screen across all monitors, with a 0/3/5/10 s countdown.
  Global hotkey is configurable (default Ctrl+Print Screen). Tray icon, single instance, and
  `--region/--window/--full/--tray` flags.
- Editor: rectangle (outline), arrow, text (with IME), highlighter, pen, crop. Shapes are
  editable (select, move, handles, recolour/resize). Undo/redo, zoom/pan. Auto-copy on open,
  Ctrl+C copies the flattened image as CF_DIB + PNG, Ctrl+S saves PNG. Settings persist in
  `%APPDATA%\Snipper\settings.ini`.
- Checks: `cargo test` (10 unit tests: geometry, doc model, settings, DIB packing),
  `cargo clippy`, `cargo build --release` (~300 KB exe). Not yet declared in Warmstart project
  settings.
- Release: `scripts/release.ps1` tests, builds and packages `dist/snipper.exe` plus a zip and
  SHA-256. Checked in Windows PowerShell 5.1 and PowerShell 7, including cargo missing from PATH
  and a running copy locking the exe (`-StopRunning`).
- Work lands on `main`. Nothing has landed yet; `C:\Dev\snipper` only has the initial commit
  until the branch is landed.

Verified by driving the debug build with window-targeted messages on a 3-monitor mixed-DPI
setup: launcher renders, full capture (5280×2560) opens the editor and lands on the clipboard,
rectangle/arrow/highlighter/text (including Korean) appear in the flattened clipboard image, a
region drag yields an exact-size snip, crop yields the cropped size, and the countdown shows
and cancels on click.

## What is unproven

- Real keyboard IME composition (Korean syllable-by-syllable display). Only WM_CHAR text was
  tested. To verify: Text tool, Hangul mode, type, and check the in-progress syllable shows at the
  caret.
- Pasting into Warmstart specifically. Clipboard formats are DIB + PNG, which Chromium/Electron
  read. To verify: copy, then paste into Warmstart.
- The global hotkey firing. On the dev machine PicPick owns every Print Screen combination, so
  registration fails and the launcher says so. To verify: quit PicPick or pick Ctrl+Alt+S under
  ⋯, then press it.
- Save dialog, custom colour dialog, close-with-unsaved prompt, and light theme have not been
  exercised.
- HDR displays: GDI capture may look washed out.

## Next

1. Hands-on pass by the user on the unproven items above. Fix what they find.
2. Declare checks in Warmstart (Project → Settings → Verification): `cargo test`,
   `cargo build --release`.
3. Nice-to-haves if wanted: an embedded .exe icon (needs rc.exe or `embed-resource`), Ctrl+V to
   paste an image into a new editor, text outline/background for readability, step-number
   badges, blur/pixelate.
