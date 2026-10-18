//! Snipper: a small screen capture and markup tool for Windows.
//!
//! `snipper` opens the launcher; `--region`, `--window` or `--full` start a
//! capture straight away; `--tray` starts hidden in the notification area.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod capture;
mod countdown;
mod doc;
mod editor;
mod geom;
mod gfx;
mod launcher;
mod output;
mod overlay;
mod settings;
mod ui;

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use settings::Mode;

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let arg = std::env::args().nth(1);
    let start = match arg.as_deref() {
        Some("--region") => Some(Mode::Region),
        Some("--window") => Some(Mode::Window),
        Some("--full") => Some(Mode::Full),
        _ => None,
    };
    let tray_only = arg.as_deref() == Some("--tray");

    // One instance owns the tray icon and hotkey; later launches forward to it.
    let _mutex = unsafe { CreateMutexW(None, false, w!(r"Local\Snipper.SingleInstance")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if let Ok(host) = unsafe { FindWindowW(launcher::CLASS, PCWSTR::null()) } {
            unsafe {
                let _ = match start {
                    Some(m) => {
                        let idx = Mode::ALL.iter().position(|x| *x == m).unwrap_or(0);
                        PostMessageW(Some(host), app::WM_APP_START, WPARAM(idx + 1), LPARAM(0))
                    }
                    None if !tray_only => PostMessageW(Some(host), app::WM_APP_SHOW, WPARAM(0), LPARAM(0)),
                    None => Ok(()),
                };
            }
        }
        return;
    }

    if let Err(e) = launcher::create(start.is_none() && !tray_only) {
        ui::message_box(None, &format!("Snipper could not start: {}", e.message()), MB_ICONERROR);
        return;
    }
    if let Some(m) = start {
        app::start(m, 0);
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
