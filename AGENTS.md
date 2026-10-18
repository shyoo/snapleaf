# snapleaf — agent guide

How to work in this codebase. Read [`HANDOFF.md`](HANDOFF.md) first: it is the only file that
carries current state.

## Before you start

- Stack: Rust (edition 2024), Windows only, MSVC toolchain. `windows` crate 0.62 for Win32,
  Direct2D, DirectWrite and WIC. No other runtime dependencies; keep it that way unless asked.
- `target/` is generated. The manifest is embedded by `build.rs` through the MSVC linker.
- Running the app pops windows on the user's desktop. Drive it with window-targeted
  `SendMessageW`/`PostMessage` rather than synthetic global input.

## Checks

Run these and make them pass before you report done:

- `cargo test`
- `cargo clippy` (no new warnings)
- `cargo build --release`

## When you commit

- Work on a branch, never directly on `main`.
- One coherent commit per task where it is safe to squash; explain *why*, not *what*.
- ⛔ Never commit generated output, dependency directories, credentials, or anything gitignored.
- Update [`HANDOFF.md`](HANDOFF.md) in the same commit: what changed, and what is next.

## House rules

- One UI thread. Per-window state is a `Box<T: ui::Handler>` on `GWLP_USERDATA`, freed on
  `WM_NCDESTROY`. App-wide state is `thread_local!`. Borrow it briefly and never across a Win32
  call that can re-enter the window procedure.
- A window that ends itself (overlay) hides, posts its result to the launcher, and posts itself
  `WM_CLOSE`. It never calls `DestroyWindow` mid-handler and then touches `self`.
- Everything renders in physical pixels (render targets at 96 DPI). UI metrics are multiplied by
  the window's `scale`. Document coordinates are image pixels.
- Unit tests live next to the code (`#[cfg(test)] mod tests`) and cover pure logic only.
- Testing pitfalls: Windows PowerShell 5.1 `Add-Type` needs `SendMessageW` (the ANSI version
  mangles non-ASCII `WM_CHAR`). Pass `[NullString]::Value`, not `$null`, for a null window title.
  pwsh 7 can't compile `System.Drawing` helpers, so use `powershell.exe`.
