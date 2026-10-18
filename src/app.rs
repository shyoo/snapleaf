//! The capture pipeline: hide Snapleaf's windows, count down, freeze the screen,
//! let the user pick, then open an editor. Timers run on the launcher window,
//! which doubles as the app's message host (tray, hotkey, single instance).

use std::cell::RefCell;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::BOOL;

use crate::capture;
use crate::gfx::Image;
use crate::settings::{self, Hotkey, Mode};
use crate::ui;
use crate::{countdown, editor, overlay};

pub const WM_APP_CAPTURED: u32 = WM_APP + 1;
pub const WM_APP_TRAY: u32 = WM_APP + 2;
/// wParam = mode index + 1 (0 = use saved mode), lParam = delay seconds.
pub const WM_APP_START: u32 = WM_APP + 3;
pub const WM_APP_SHOW: u32 = WM_APP + 4;
pub const WM_APP_CANCEL: u32 = WM_APP + 5;

pub const TIMER_TICK: usize = 1;
pub const TIMER_GRAB: usize = 2;
pub const HOTKEY_ID: i32 = 1;

#[derive(Default)]
struct Pipeline {
    host: HWND,
    busy: bool,
    mode: Option<Mode>,
    remaining: u32,
    countdown: Option<HWND>,
    restore_launcher: bool,
    hidden: Vec<HWND>,
}

thread_local! {
    static PIPE: RefCell<Pipeline> = RefCell::new(Pipeline::default());
}

pub fn set_host(hwnd: HWND) {
    PIPE.with(|p| p.borrow_mut().host = hwnd);
}

pub fn host() -> HWND {
    PIPE.with(|p| p.borrow().host)
}

pub fn start(mode: Mode, delay: u32) {
    let host = host();
    let already = PIPE.with(|p| std::mem::replace(&mut p.borrow_mut().busy, true));
    if already {
        return;
    }
    let hidden = visible_app_windows();
    let launcher_visible = hidden.contains(&host);
    for &hwnd in &hidden {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    let cd = if delay > 0 { countdown::show(delay).ok() } else { None };
    PIPE.with(|p| {
        let mut p = p.borrow_mut();
        p.mode = Some(mode);
        p.remaining = delay;
        p.countdown = cd;
        p.restore_launcher = launcher_visible;
        p.hidden = hidden;
    });
    unsafe {
        if delay > 0 {
            SetTimer(Some(host), TIMER_TICK, 1000, None);
        } else {
            // Give the exposed applications time to repaint before freezing.
            SetTimer(Some(host), TIMER_GRAB, 220, None);
        }
    }
}

pub fn on_timer(id: usize) {
    if !is_busy() {
        return;
    }
    let host = host();
    match id {
        TIMER_TICK => {
            let (left, cd) = PIPE.with(|p| {
                let mut p = p.borrow_mut();
                p.remaining = p.remaining.saturating_sub(1);
                (p.remaining, p.countdown)
            });
            if left == 0 {
                unsafe {
                    let _ = KillTimer(Some(host), TIMER_TICK);
                    let cd = PIPE.with(|p| p.borrow_mut().countdown.take());
                    if let Some(cd) = cd {
                        let _ = DestroyWindow(cd);
                    }
                    SetTimer(Some(host), TIMER_GRAB, 150, None);
                }
            } else if let Some(cd) = cd {
                countdown::set(cd, left);
            }
        }
        TIMER_GRAB => {
            unsafe {
                let _ = KillTimer(Some(host), TIMER_GRAB);
            }
            let mode = PIPE.with(|p| p.borrow().mode).unwrap_or(Mode::Region);
            // Synchronize with the compositor after hiding our windows and countdown.
            unsafe {
                let _ = DwmFlush();
            }
            match capture::freeze() {
                Ok(frozen) if mode == Mode::Full => finish(Some(frozen.img)),
                Ok(frozen) => {
                    if overlay::show(frozen, mode).is_err() {
                        finish(None);
                    }
                }
                Err(e) => {
                    finish(None);
                    ui::message_box(None, &format!("Screen capture failed: {}", e.message()), MB_ICONERROR);
                }
            }
        }
        _ => {}
    }
}

/// Abort a pending countdown (the user clicked it).
pub fn cancel() {
    if !is_busy() {
        return;
    }
    let host = host();
    let cd = PIPE.with(|p| p.borrow_mut().countdown.take());
    unsafe {
        let _ = KillTimer(Some(host), TIMER_TICK);
        let _ = KillTimer(Some(host), TIMER_GRAB);
        if let Some(cd) = cd {
            let _ = DestroyWindow(cd);
        }
    }
    finish(None);
}

pub fn is_busy() -> bool {
    PIPE.with(|p| p.borrow().busy)
}

/// End of the pipeline: open an editor, or bring the launcher back on cancel.
pub fn finish(img: Option<Image>) {
    let (restore, hidden) = PIPE.with(|p| {
        let mut p = p.borrow_mut();
        p.busy = false;
        (p.restore_launcher, std::mem::take(&mut p.hidden))
    });
    // Back to front, without activation: preserve existing editors and leave
    // focus available for the new editor (or the application beneath us).
    for hwnd in hidden.into_iter().rev().filter(|hwnd| *hwnd != host()) {
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOWNA);
            }
        }
    }
    match img {
        Some(img) if img.w > 0 && img.h > 0 => {
            if let Err(e) = editor::open(img) {
                if restore {
                    show_launcher();
                }
                ui::message_box(None, &format!("Could not open the editor: {}", e.message()), MB_ICONERROR);
            }
        }
        _ => {
            if restore {
                show_launcher();
            }
        }
    }
}

pub fn show_launcher() {
    if is_busy() {
        return;
    }
    let host = host();
    unsafe {
        let _ = ShowWindow(host, SW_SHOWNORMAL);
        let _ = SetForegroundWindow(host);
    }
}

/// All our top-level windows live on this UI thread. Collect first so no
/// application-state borrow or enumeration callback spans ShowWindow reentry.
fn visible_app_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
        unsafe {
            if IsWindowVisible(hwnd).as_bool() {
                (&mut *(lp.0 as *mut Vec<HWND>)).push(hwnd);
            }
        }
        BOOL(1)
    }
    let mut windows = Vec::<HWND>::new();
    unsafe {
        let _ = EnumThreadWindows(GetCurrentThreadId(), Some(collect), LPARAM(&mut windows as *mut _ as isize));
    }
    windows
}

/// (Re)register the global hotkey from settings. Returns false if another
/// program already owns it.
pub fn register_hotkey() -> bool {
    let host = host();
    unsafe {
        let _ = UnregisterHotKey(Some(host), HOTKEY_ID);
        let (mods, vk) = match settings::get().hotkey {
            Hotkey::CtrlPrintScreen => (MOD_CONTROL, VK_SNAPSHOT.0 as u32),
            Hotkey::PrintScreen => (HOT_KEY_MODIFIERS(0), VK_SNAPSHOT.0 as u32),
            Hotkey::CtrlAltS => (MOD_CONTROL | MOD_ALT, 'S' as u32),
            Hotkey::CtrlShiftS => (MOD_CONTROL | MOD_SHIFT, 'S' as u32),
            Hotkey::Off => return true,
        };
        RegisterHotKey(Some(host), HOTKEY_ID, mods | MOD_NOREPEAT, vk).is_ok()
    }
}

/// Saved mode, but never with a delay: the hotkey means "now".
pub fn start_from_hotkey() {
    start(settings::get().mode, 0);
}

pub fn start_saved() {
    let s = settings::get();
    start(s.mode, s.delay);
}
