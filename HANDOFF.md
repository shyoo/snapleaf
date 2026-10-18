# Snapleaf - Handoff

**Current state and what to do next, not a changelog.**

## Where things stand

The app is now **Snapleaf**. Cargo package/binary, manifest, Win32 window classes,
single-instance mutex, UI titles, tray messages, build resources and release packaging
use the new name. Releases are `dist/snapleaf.exe`, `snapleaf-0.1.0-x64.zip` and SHA-256.
Public repository: https://github.com/shyoo/snapleaf (origin). Main currently has the
Snapleaf rename at f3c9ddd8. The main checkout is C:\Dev\snapleaf; Git worktree links
were repaired and Warmstart is bound to the new directory.

This task branch prepares the public docs and release automation:
- README focuses on download, capture, annotation and sharing, with three real screenshots
  of the launcher, region selection and editor. The demo includes Snapleaf's logo and a
  captured launcher image. No AI-generated UI is used.
- Contributor details, icon assets, settings and release instructions moved to
  docs/development.md. Apache 2.0 LICENSE and Cargo license/repository metadata added.
- scripts/release.ps1 uses the lockfile and includes LICENSE in the ZIP.
- .github/workflows/release.yml tests, lints and packages on a GitHub-hosted Windows x64
  runner for branch pushes, PRs and manual runs; artifacts last 14 days. Matching annotated
  version tags on main also publish EXE, ZIP and checksum as a GitHub Release. Hyphenated
  versions become prereleases. No release tag has been created for this task.
- Local checks: 10 tests pass; Clippy has only the two existing warnings; release packaging,
  ZIP contents/checksum, screenshot inspection and actionlint pass. Remote build verification
  follows the task-branch push; see its Actions run for authoritative status.

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

## Workspace

This branch lives in a Warmstart-managed pooled worktree. The earlier rename landed and
was pushed to origin/main. Do not edit Warmstart's project.json manually. The current
finish policy is await-human, local commit-and-merge; publishing this task branch for
runner verification does not land it on main.

## What is unproven

- Real keyboard IME composition (only WM_CHAR text was tested previously).
- Global hotkey on this machine: PicPick owns Print Screen combinations. Quit the older
  Snipper before trying Snapleaf so the two apps do not compete for the shortcut.
- Save/custom colour/unsaved prompt/light theme and unusual GDI-to-DirectWrite font mappings
  have not had full live coverage.
- Windows tray pinning belongs to the executable path. The previously pinned old release
  will not automatically pin the new executable.

## Next

1. Land the public docs/license/release workflow when requested, then push main as authorized.
2. Create a matching annotated version tag when a public release is requested.
3. Complete live app checks listed above as needed.
