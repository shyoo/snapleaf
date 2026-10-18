# Snapleaf - Handoff

**Current state and what to do next, not a changelog.**

## Where things stand

Public repository: https://github.com/shyoo/snapleaf (origin). Main checkout:
C:\Dev\snapleaf, with repaired worktree links. Main has the Snapleaf rename at
f3c9ddd8. This task branch also contains public docs, Apache 2.0 licensing,
three real app screenshots, and the installer/release/update work below.

- Native Rust/Win32 screenshot and markup app: region/window/full capture, countdown,
  configurable hotkey, tray, single instance, editable annotations, undo/redo, crop,
  zoom/pan, clipboard DIB+PNG, save PNG, text with IME and fonts.
- Branding is shared by app/tray/window icons and favicons in assets/. Regenerate with
  scripts/new-app-icon.ps1 in Windows PowerShell 5.1; keep SVG geometry aligned.
- README is user-facing; contributor and release details live in docs/development.md.
- Inno Setup 6.7.0 per-user x64 installer: fixed app ID and installation directory,
  Start menu/optional desktop shortcuts, uninstaller, LICENSE. Refuses install/uninstall
  while the single-instance mutex exists; never force-closes editors. Portable ZIP stays.
- scripts/release.ps1 -Installer builds setup and portable files with SHA256SUMS.txt.
  -Version / SNAPLEAF_VERSION supplies a release version without Cargo file edits;
  build.rs embeds it in the executable. Cargo's 0.1.0 remains the development baseline.
- .github/workflows/ci.yml tests/lints/packages and smoke-tests installation on Windows.
  release.yml runs on annotated version tags (or manual artifact-only builds), verifies
  ancestry and successful main CI, builds and publishes setup/portable/checksum assets
  with tag-message notes and build attestations. RCs are prereleases, finals are latest.
- .claude/skills/release/SKILL.md supports /release rc and /release promote, backed by
  scripts/release-tag.ps1. RCs tag origin/main; promotion tags the open RC's exact source
  commit even if main moved. Final rebuild embeds final version (not byte-identical RC).
  Helper checks clean files, published local main, increasing versions, main CI, and
  published candidate assets before promotion. No tags/releases have been cut.
- Release builds check stable GitHub releases after 30s and every six hours, download
  the matching setup with checksum verification, then notify. User opens installer
  from the menu; SHA-256 is rechecked. No silent installation or app termination.
  Manual check/automatic-download toggle in launcher and tray. Debug checks are manual.
- Updater runs embedded PowerShell using OS HTTP/JSON/crypto libraries on a worker thread;
  no runtime dependencies added. HTTPS GitHub/CDN redirects only, bounded size/time,
  partial files staged before verification, current/downgrade/RC releases filtered.
- Settings: %APPDATA%\Snapleaf\settings.ini, legacy Snipper fallback if absent.
  Update toggle: %APPDATA%\Snapleaf\updates-disabled; downloads:
  %LOCALAPPDATA%\Snapleaf\updates. Installer: %LOCALAPPDATA%\Programs\Snapleaf.

## Verification

Local cargo test (10), cargo build --release and cargo clippy pass; only the two
existing chunks_exact warnings. scripts/test-release.ps1 passes offline version,
release planning/promotion, URL, checksum and download fixtures. Skill validation
and actionlint pass. Candidate and final setup files compile. test-installer.ps1
passes real per-user install, running-app refusal, registry version, payload hash,
LICENSE and uninstall checks in an isolated target/ directory. The embedded worker
command was exercised against GitHub; no stable release currently exists.
Remote CI verification follows the task-branch push; consult its Actions run.

## Workspace and next

Warmstart-managed pooled worktree; await-human, local commit-and-merge finish policy.
Do not edit project.json, merge/push main yourself, or complete the task without request.

1. Land when requested, then publish main through the authorized project flow.
2. After main CI passes, /release rc; install and verify it, then /release promote.
3. Verify real stable-release download/open/upgrade after the first final is published.

## What is unproven

- End-to-end GitHub stable update and RC-to-final upgrade with published assets.
- Real keyboard IME composition, save/custom colour/unsaved prompt/light theme,
  unusual font mappings, and live global hotkey (PicPick may own Print Screen).
- Installer and binaries are unsigned; attestations do not provide Windows signing.
- Tray pinning belongs to the executable path; old pins do not migrate automatically.
