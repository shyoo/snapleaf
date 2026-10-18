//! Stable release polling and verified installer downloads, off the UI thread.
use crate::{launcher, ui};
use std::{
    cell::RefCell,
    io::Write,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::MB_ICONINFORMATION};

pub const TIMER: usize = 10;
const VERSION: &str = env!("SNAPLEAF_VERSION");

struct Updates {
    pending: Option<Receiver<Result<String, String>>>,
    ready: Option<(String, String)>,
    last_check: Instant,
    manual: bool,
}
thread_local! {
    static STATE: RefCell<Updates> = RefCell::new(Updates {
        pending: None, ready: None, last_check: Instant::now() - Duration::from_secs(21600 - 30), manual: false,
    });
}

fn disabled_path() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("Snapleaf/updates-disabled")
}
pub fn enabled() -> bool {
    !disabled_path().exists()
}
pub fn toggle() {
    let path = disabled_path();
    if path.exists() {
        let _ = std::fs::remove_file(path);
    } else if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(path, b"");
    }
}
pub fn label() -> String {
    STATE.with(|s| {
        let s = s.borrow();
        if s.pending.is_some() {
            "Checking / downloading update…".into()
        } else if s.ready.is_some() {
            "Open downloaded update installer…".into()
        } else {
            format!("Check for updates… (v{VERSION})")
        }
    })
}

fn worker(open: Option<(String, String)>) -> Result<String, String> {
    let system = std::env::var_os("SystemRoot").ok_or("SystemRoot missing")?;
    let mut cmd =
        Command::new(PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "-",
    ])
    .creation_flags(0x08000000)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .env("SNAPLEAF_CURRENT_VERSION", VERSION)
    .env_remove("SNAPLEAF_UPDATE_OPEN")
    .env_remove("SNAPLEAF_UPDATE_HASH");
    if let Some((path, hash)) = open {
        cmd.env("SNAPLEAF_UPDATE_OPEN", path)
            .env("SNAPLEAF_UPDATE_HASH", hash);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    // A single script block makes stdin execute as one unit in Windows PowerShell.
    let script = format!("& {{\n{}\n}}\n\n", include_str!("../scripts/update.ps1"));
    child
        .stdin
        .take()
        .ok_or("Missing worker stdin")?
        .write_all(script.as_bytes())
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn check(manual: bool) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.pending.is_some() {
            return;
        }
        let open = if manual { s.ready.take() } else { None };
        let (tx, rx) = mpsc::channel();
        s.pending = Some(rx);
        s.manual = manual;
        s.last_check = Instant::now();
        std::thread::spawn(move || {
            let _ = tx.send(worker(open));
        });
    });
}

pub fn tick(hwnd: HWND) {
    let (result, manual, due) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let result = s.pending.as_ref().and_then(|r| r.try_recv().ok());
        if result.is_some() {
            s.pending = None;
        }
        (
            result,
            s.manual,
            s.pending.is_none()
                && s.ready.is_none()
                && s.last_check.elapsed() >= Duration::from_secs(21600),
        )
    });
    if let Some(result) = result {
        let message = match result {
            Ok(text) if text.starts_with("ready\t") => {
                let parts: Vec<_> = text.split('\t').collect();
                if parts.len() == 3 {
                    STATE.with(|s| s.borrow_mut().ready = Some((parts[1].into(), parts[2].into())));
                    "An update is downloaded and verified. Use the update menu to open the installer. Save your images and exit Snapleaf before installing.".to_owned()
                } else { "Unexpected update response.".into() }
            }
            Ok(text) if text == "current" => "Snapleaf is up to date.".into(),
            Ok(text) if text == "unpublished" => "No stable Snapleaf release has been published yet.".into(),
            Ok(text) if text == "opened" => "Installer opened. Save your images, then exit Snapleaf from its tray menu to continue installing.".into(),
            Ok(_) => "Unexpected update response. Please check again.".into(),
            Err(e) => format!("Could not check or open the update: {e}"),
        };
        if manual {
            ui::message_box(Some(hwnd), &message, MB_ICONINFORMATION);
        } else if STATE.with(|s| s.borrow().ready.is_some()) {
            launcher::tray_balloon(hwnd, &message);
        }
    }
    if due && enabled() && !cfg!(debug_assertions) {
        check(false);
    }
}
