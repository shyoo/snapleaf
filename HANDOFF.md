# Snapleaf - Handoff

**Current state and what to do next, not a changelog.**

## Where things stand

The app is now **Snapleaf**. Cargo package/binary, manifest, Win32 window classes,
single-instance mutex, UI titles, tray messages, build resources and release packaging
use the new name. Releases are `dist/snapleaf.exe`, `snapleaf-0.1.0-x64.zip` and SHA-256.
README includes the logo, usage, build instructions, shortcuts and favicon asset links.

- Branding: `assets/snapleaf.svg` is the leaf/capture-frame design. PNGs cover favicons
  (16/32), touch (180), and project images (192/512). `snapleaf.ico` contains nine frames
  from 16 to 256 px; `favicon.ico` contains 16/32/48. Recreate raster assets using
  Windows PowerShell 5.1: `scripts/new-app-icon.ps1`. Keep its geometry aligned with the SVG.
- Window/tray icons load the embedded application icon at the system DPI size; no duplicate
  runtime artwork. `build.rs` still embeds resources without a resource compiler.
- Preferences save under `%APPDATA%\Snapleaf\settings.ini`. When unavailable, preferences
  are read from the legacy `%APPDATA%\Snipper\settings.ini`; the old file is untouched.
- Capture: rectangle, window and full screen across monitors, optional 0/3/5/10 s countdown.
  Configurable hotkey (default Ctrl+Print Screen), tray, single instance, CLI flags.
  Launcher and editors hide during capture and return afterward without activation.
- Editor: rectangle, arrow, text with IME and font choices, highlighter, pen, crop;
  editable shapes, undo/redo, zoom/pan, auto-copy, Ctrl+C flattened DIB+PNG, Ctrl+S PNG.
- Checks: 10 unit tests pass; cargo clippy has two existing chunks_exact style warnings
  and no new warnings; cargo build --release and scripts/release.ps1 pass. Windows loaded
  all nine embedded icon sizes; PNG dimensions and release ZIP entries were checked.

## Workspace and directory rename

This branch lives in a Warmstart-managed pooled worktree. The user authorized landing,
then moving the main checkout from `C:\Dev\snipper` to `C:\Dev\snapleaf`.
After the move, repair Git worktree links and verify them. The user will rebind the
project in Warmstart when prompted; do not edit Warmstart's project.json manually.
No GitHub remote is configured, and nothing has been published.

## What is unproven

- Real keyboard IME composition (only WM_CHAR text was tested previously).
- Global hotkey on this machine: PicPick owns Print Screen combinations. Quit the older
  Snipper before trying Snapleaf so the two apps do not compete for the shortcut.
- Save/custom colour/unsaved prompt/light theme and unusual GDI-to-DirectWrite font mappings
  have not had full live coverage.
- Windows tray pinning belongs to the executable path. The previously pinned old release
  will not automatically pin the new executable.

## Next

1. Land the Snapleaf rename, move the main checkout and repair Git worktree links.
2. User rebinds Warmstart to `C:\Dev\snapleaf` when prompted.
3. User tries Snapleaf; configure a public GitHub repository when requested.
