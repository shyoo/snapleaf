---
name: release
description: Cut a Snapleaf release candidate with /release rc (optional patch, minor or major), or promote the tested candidate with /release promote. The annotated tag supplies the version and release notes; no version-bump commit.
---

Use this workflow when the user asks to cut or promote a release. Creating this skill does not itself authorize publishing a release.

Run from the repository root with PowerShell and authenticated `gh`:

```powershell
./scripts/release-tag.ps1 -Action plan -Release rc
./scripts/release-tag.ps1 -Action plan -Release rc -Bump minor
./scripts/release-tag.ps1 -Action plan -Release promote
```

`rc` uses origin/main, continues an open candidate series, otherwise bumps patch (the first series uses Cargo's baseline). Use minor/major only if requested. `promote` uses the highest open candidate's own commit even if main has advanced. It builds that source with the final version embedded. Cargo.toml and Cargo.lock remain unchanged.

Read the plan's commit and changes since `Since` using git log/diff. For promotion, start with `git for-each-ref --format='%(contents)' refs/tags/<candidate>`. Write concise user-facing notes in a file under gitignored `dist/`, covering changes, upgrade steps when needed, and known problems. The tag's entire message becomes the GitHub release notes.

```powershell
./scripts/release-tag.ps1 -Action cut -Release rc -NotesFile dist/release-notes.md -DryRun
./scripts/release-tag.ps1 -Action cut -Release rc -NotesFile dist/release-notes.md
# Or use -Release promote for both calls. Preserve the requested -Bump on both calls.
```

The helper refuses dirty files, unpublished local main commits, versions that go backward, and commits without successful main CI. Promotion also requires a published candidate installer and checksum manifest. Stop and report a failed gate; don't force tags, bypass CI, commit unrelated changes, or push main to clear a gate. In a Warmstart-managed checkout use its landing flow when the user asks to land work.

After publishing the tag, find its run with `gh run list --workflow release.yml`, watch that run to completion with `gh run watch <id> --exit-status`, and report its URL, tag, source commit, and release URL. A failed workflow is a failed release; report its error without creating replacement tags automatically. RCs are prereleases: install and verify one before `/release promote`. Finals become latest and are offered to installed apps. Never claim runtime installation verification from CI alone.
