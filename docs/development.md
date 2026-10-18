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

`./scripts/release.ps1` tests, builds and packages the portable EXE and ZIP under gitignored
`dist/`. Add `-Installer` to compile the per-user setup with Inno Setup 6.7.0 (put ISCC.exe
on PATH). `-Version 0.2.0-rc.1` overrides the Cargo baseline for this build only; the same
version is embedded in the app and used in installer metadata and filenames. No Cargo files
are modified. `-SkipTests` skips unit tests; `-StopRunning` stops copies from this checkout.

The ZIP includes README and LICENSE. `SHA256SUMS.txt` covers the EXE, ZIP, and installer;
the ZIP also has its own `.sha256`. Binaries and installers are unsigned.

Installer identity and directory stay fixed across candidates and final versions:
`%LOCALAPPDATA%/Programs/Snapleaf`. It registers an uninstaller and shortcuts, preserves
preferences, and refuses installation/uninstallation while Snapleaf is running. Save your
images and exit through the tray first. It does not add a startup task or force-close editors.

Settings are stored in `%APPDATA%/Snapleaf/settings.ini`. If absent, the app reads
legacy `%APPDATA%/Snipper/settings.ini` preferences; subsequent saves use Snapleaf.
CLI flags: `--region`, `--window`, `--full`, and `--tray`.

## GitHub Actions and releases

[CI](../.github/workflows/ci.yml) tests, lints and packages the portable app and installer
on GitHub-hosted Windows x64 runners for branch pushes, PRs and manual runs. Artifacts last
14 days. No self-hosted runner or repository secrets are needed.

[Release](../.github/workflows/release.yml) builds annotated version tags on published main
commits with successful main CI. The tag supplies the version (`SNAPLEAF_VERSION`) and its
message supplies the release notes. Cargo's version remains the development baseline.
Published assets include the setup installer, portable EXE/ZIP, checksums and GitHub build
provenance attestations. Manual runs only upload artifacts. Only publishing gets write access.

Use the repo's [.claude release skill](../.claude/skills/release/SKILL.md): `/release rc`
(default patch; optional `minor` or `major`), install and verify the candidate, then
`/release promote`. The scripts are also directly usable:

```powershell
./scripts/release-tag.ps1 -Action plan -Release rc
# Write user-facing notes to dist/release-notes.md, then:
./scripts/release-tag.ps1 -Action cut -Release rc -NotesFile dist/release-notes.md -DryRun
./scripts/release-tag.ps1 -Action cut -Release rc -NotesFile dist/release-notes.md
# After installing and verifying the candidate:
./scripts/release-tag.ps1 -Action plan -Release promote
./scripts/release-tag.ps1 -Action cut -Release promote -NotesFile dist/release-notes.md -DryRun
./scripts/release-tag.ps1 -Action cut -Release promote -NotesFile dist/release-notes.md
```

The helper fetches origin/main and tags, refuses dirty worktrees, unpublished local main
commits, non-increasing versions and missing main CI. Promotion requires a published RC
installer/checksums and tags the RC's own source commit even if main has advanced. A final
rebuild embeds the final version, so the bytes differ from the RC. It never commits or pushes
main. Land/publish work before cutting candidates. RCs are prereleases with `latest=false`;
final tags publish as latest. Nothing publishes merely by adding this automation.

## Updates

Release builds poll GitHub's public stable `/releases/latest` endpoint after 30 seconds,
then every six hours while running. Debug builds only check on demand. The menus offer a
manual check and an automatic-download toggle (persisted as
`%APPDATA%/Snapleaf/updates-disabled`). No token or GitHub login is needed. Network errors
stay quiet for automatic checks; manual checks show errors. With no stable release, the menu
reports that none has been published yet.

A worker thread runs the embedded `scripts/update.ps1` in hidden Windows PowerShell 5.1;
HTTP/JSON/SHA-256 use OS libraries, with no additional runtime dependency. Only HTTPS
GitHub/asset-CDN URLs are accepted, including redirects. Responses and download time/size
are bounded. Releases must be stable and newer; candidate-to-final ordering is supported.
The exact versioned x64 setup asset and `SHA256SUMS.txt` must be present. Downloads are
staged as `.part` files under `%LOCALAPPDATA%/Snapleaf/updates` and renamed only after
verification; an existing verified installer is reused. Its checksum is checked again before
opening it on the user's request. Snapleaf never silently runs an installer or exits editors.
Portable users can install through this flow or replace their portable files themselves.

`./scripts/test-release.ps1` runs offline version, URL and checksum tests plus mocked download
fixtures covering verified download reuse, mismatches, prerelease filtering and current
versions. Actual GitHub update/install coverage needs a published stable release; attestations
are available through `gh attestation verify`, and are not verified by the updater.
Checksums protect download integrity and rely on the GitHub repository as the trust source.

## Documentation screenshots

The README images show the running application, not mock UI. The capture subject is a demo
window containing Snapleaf's own logo and a screenshot of its launcher. No personal desktop
content is included. See `docs/screenshots/` for the launcher, region selection, and annotated
editor images. The app hides itself during capture, so the demo displays a previously captured
launcher image to demonstrate annotating Snapleaf itself.

