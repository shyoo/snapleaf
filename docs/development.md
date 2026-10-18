# Development

Snapleaf uses Rust 2024, Win32, Direct2D, DirectWrite and WIC. All rendering uses physical pixels.

## Icon and favicon assets

The Snapleaf mark pairs a pale green leaf with white capture corners on a green tile.
The same icon appears in Explorer, the tray, and window title bars.

| Asset | Use |
| --- | --- |
| [`snapleaf.svg`](../assets/snapleaf.svg) | Scalable logo |
| [`snapleaf.ico`](../assets/snapleaf.ico) | Windows app icon, 16–256 px |
| [`favicon.ico`](../assets/favicon.ico) | Web favicon, 16/32/48 px |
| [`favicon-16x16.png`](../assets/favicon-16x16.png), [`favicon-32x32.png`](../assets/favicon-32x32.png) | PNG favicons |
| [`apple-touch-icon.png`](../assets/apple-touch-icon.png) | 180 px touch icon |
| [`snapleaf-192.png`](../assets/snapleaf-192.png), [`snapleaf-512.png`](../assets/snapleaf-512.png) | Larger app and project images |

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


## Local packaging

Run `powershell -ExecutionPolicy Bypass -File scripts/release.ps1` to test, build,
and package `dist/snapleaf.exe`, `snapleaf-<version>-x64.zip` and its SHA-256 checksum.
The ZIP includes README and LICENSE. `-SkipTests` skips unit tests; `-StopRunning` exits
copies running from this checkout's release or dist directories before rebuilding.

Settings are stored in `%APPDATA%\Snapleaf\settings.ini`. If absent, the app reads
legacy `%APPDATA%\Snipper\settings.ini` preferences; subsequent saves use Snapleaf.
CLI flags: `--region`, `--window`, `--full`, and `--tray`.

## GitHub Actions and releases

[Build and release](../.github/workflows/release.yml) uses a GitHub-hosted Windows x64 runner.
Every branch push, pull request, and manual run tests, lints, and builds the portable package.
The `snapleaf-windows-x64` artifact contains the EXE, ZIP, and ZIP checksum (14-day retention).
No self-hosted runner or repository secrets are needed.

To publish a release, set the Cargo.toml version, land that commit on main, and
refresh Cargo.lock if needed, then create and push an annotated matching version tag:

```powershell
git tag -a v0.1.0 -m "Snapleaf 0.1.0"
git push origin v0.1.0
```

The runner rejects tags that do not match Cargo.toml, are lightweight, or point outside
main. Only a successful tag build publishes a GitHub Release with the EXE, ZIP, and checksum.
Versions containing a hyphen (for example `0.2.0-rc.1`) become prereleases.
Manual runs only upload artifacts. Release notes are generated from GitHub's commit/PR history.
Only the publish job has permission to write repository contents. Binaries are unsigned.

## Documentation screenshots

The README images show the running application, not mock UI. The capture subject is a demo
window containing Snapleaf's own logo and a screenshot of its launcher. No personal desktop
content is included. See `docs/screenshots/` for the launcher, region selection, and annotated
editor images. The app hides itself during capture, so the demo displays a previously captured
launcher image to demonstrate annotating Snapleaf itself.

