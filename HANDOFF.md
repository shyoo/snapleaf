# snipper — Handoff

**Current state and what to do next, not a changelog.** Replaced as work lands, never stacked.

## Where things stand

v1 of the capture + markup tool is on `main` and the user reports it works well. This branch
adds a font choice for text annotations. See README for features and layout.

- Capture: rectangle, window and full screen across all monitors, with a 0/3/5/10 s countdown.
  Global hotkey is configurable (default Ctrl+Print Screen). Tray icon, single instance, and
  `--region/--window/--full/--tray` flags.
- Editor: rectangle (outline), arrow, text (with IME), highlighter, pen, crop. Shapes are
  editable (select, move, handles, recolour/resize). Undo/redo, zoom/pan. Auto-copy on open,
  Ctrl+C copies the flattened image as CF_DIB + PNG, Ctrl+S saves PNG. Settings persist in
  `%APPDATA%\Snipper\settings.ini`.
- Fonts: each text annotation carries a `FontSpec` (DirectWrite family, bold, italic). The
  toolbar font button lists curated installed fonts plus Bold/Italic and **More fonts…** (the
  ChooseFont dialog, mapped to a DirectWrite family via GDI interop). Ctrl+B and Ctrl+I toggle.
  It applies to the selected text, or to the default for new text. Restyling while typing keeps
  the edit open. The default font is Segoe UI Bold; it was semibold before, so default text
  looks slightly heavier.
- Tray: Windows 11 hides new tray icons in the ^ overflow until they're pinned. Pinning is per
  exe path (`HKCU\Control Panel\NotifyIconSettings\*\IsPromoted`). The user chose a one-time pin
  of `C:\Dev\snipper\dist\snipper.exe`, done by hand, not by the app. Tray and window icons are
  now drawn at the system icon size for the primary display's DPI instead of a stretched 16 px.
- Checks: the Warmstart check is `scripts/release.ps1 -StopRunning` (cargo test + release build). Also run
  `cargo clippy`; the three remaining warnings are pre-existing style ones.
- Release: `scripts/release.ps1` tests, builds and packages `dist/snipper.exe` plus a zip and
  SHA-256.

Font change was verified live by driving the font menu and the font dialog: Georgia, then
italic, then Consolas through More fonts…. The result rendered on screen and in the flattened
clipboard image, Korean fell back cleanly, and the setting persisted.

## What is unproven

- Real keyboard IME composition (Korean syllable-by-syllable display). Only WM_CHAR text was
  tested.
- The global hotkey firing. PicPick on the dev machine owns every Print Screen combination.
- Save dialog, custom colour dialog, close-with-unsaved prompt, and light theme have not been
  exercised by the agent.
- Fonts whose DirectWrite family name differs from the GDI face, beyond the common Windows
  fonts. If one falls back to the default face, the GDI-interop mapping in `fonts::choose` is
  where to look.

## Next

1. User tries the font button. Fix what they find.
2. Nice-to-haves if wanted: an embedded .exe icon, Ctrl+V to paste an image into a new editor,
   text outline/background for readability, free font size, step-number badges, blur/pixelate.
