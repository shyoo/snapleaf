//! The launcher bar (New · mode · delay · more), the tray icon, and the
//! message host for timers, the global hotkey and single-instance requests.

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::app;
use crate::geom::*;
use crate::gfx::*;
use crate::settings::{self, DELAYS, Hotkey, Mode};
use crate::ui::{self, Button, ButtonState, Handler, Icon, Look, WindowSpec};

pub const CLASS: PCWSTR = w!("SnapleafLauncher");

const CLIENT_W: f32 = 430.0;
const CLIENT_H: f32 = 92.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Btn {
    New,
    Mode,
    Delay,
    More,
}

// Menu command ids.
const CMD_MODE: usize = 100;
const CMD_DELAY: usize = 200;
const CMD_HOTKEY: usize = 300;
const CMD_SNIP: usize = 400;
const CMD_SHOW: usize = 500;
const CMD_EXIT: usize = 900;

struct Launcher {
    target: WindowTarget,
    scale: f32,
    hover: Option<Btn>,
    pressed: Option<Btn>,
    hotkey_ok: bool,
    taskbar_created: u32,
    told_about_tray: bool,
}

pub fn create(show: bool) -> Result<HWND> {
    let (work, scale) = ui::cursor_monitor();
    let (w, h) = window_size(scale, (scale * 96.0) as u32);
    let x = (work.center().x - w as f32 / 2.0) as i32;
    let y = (work.t + work.h() * 0.2) as i32;
    let hwnd = ui::create_window(
        WindowSpec {
            class: CLASS,
            title: "Snapleaf",
            style: WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            ex: WINDOW_EX_STYLE(0),
            rect: (x, y, w, h),
            cursor: IDC_ARROW,
            owner: None,
        },
        Launcher {
            target: WindowTarget::default(),
            scale,
            hover: None,
            pressed: None,
            hotkey_ok: true,
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
            told_about_tray: false,
        },
    )?;
    app::set_host(hwnd);
    ui::style_frame(hwnd, ui::theme().dark);
    let ok = app::register_hotkey();
    unsafe {
        SendMessageW(hwnd, WM_USER, Some(WPARAM(ok as usize)), None);
    }
    add_tray(hwnd);
    if show {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
    }
    Ok(hwnd)
}

fn window_size(scale: f32, dpi: u32) -> (i32, i32) {
    let mut r = RECT { left: 0, top: 0, right: (CLIENT_W * scale) as i32, bottom: (CLIENT_H * scale) as i32 };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX, false, WINDOW_EX_STYLE(0), dpi);
    }
    (r.right - r.left, r.bottom - r.top)
}

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: app::WM_APP_TRAY,
        hIcon: ui::app_icon(ui::small_icon_size()),
        ..Default::default()
    };
    copy_wide(&mut nid.szTip, &format!("Snapleaf — {}", hotkey_hint()));
    nid
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

fn add_tray(hwnd: HWND) {
    let nid = tray_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

fn update_tray_tip(hwnd: HWND) {
    let nid = tray_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

fn remove_tray(hwnd: HWND) {
    let nid = tray_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn tray_balloon(hwnd: HWND, text: &str) {
    let mut nid = tray_data(hwnd);
    nid.uFlags |= NIF_INFO;
    copy_wide(&mut nid.szInfoTitle, "Snapleaf is still running");
    copy_wide(&mut nid.szInfo, text);
    nid.dwInfoFlags = NIIF_INFO;
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

fn hotkey_hint() -> String {
    match settings::get().hotkey {
        Hotkey::Off => "no global shortcut".into(),
        h => format!("{} to snip", h.label()),
    }
}

fn delay_label(d: u32) -> String {
    if d == 0 { "No delay".into() } else { format!("{d} s delay") }
}

fn mode_icon(m: Mode) -> Icon {
    match m {
        Mode::Region => Icon::ModeRegion,
        Mode::Window => Icon::ModeWindow,
        Mode::Full => Icon::ModeFull,
    }
}

/// Menu entries shared by the tray and the "more" button.
fn hotkey_items() -> Vec<(usize, String, bool)> {
    let cur = settings::get().hotkey;
    let mut v = vec![(1, "»»Global shortcut".to_string(), false)];
    for (i, h) in Hotkey::ALL.iter().enumerate() {
        v.push((CMD_HOTKEY + i, format!("»{}", h.label()), *h == cur));
    }
    v
}

/// Handle a menu command from any of the launcher's menus.
pub fn run_command(hwnd: HWND, cmd: usize) {
    match cmd {
        c if (CMD_MODE..CMD_MODE + 3).contains(&c) => settings::update(|s| s.mode = Mode::ALL[c - CMD_MODE]),
        c if (CMD_DELAY..CMD_DELAY + DELAYS.len()).contains(&c) => settings::update(|s| s.delay = DELAYS[c - CMD_DELAY]),
        c if (CMD_HOTKEY..CMD_HOTKEY + Hotkey::ALL.len()).contains(&c) => {
            let before = settings::get().hotkey;
            let chosen = Hotkey::ALL[c - CMD_HOTKEY];
            settings::update(|s| s.hotkey = chosen);
            let ok = app::register_hotkey();
            if !ok {
                ui::message_box(
                    Some(hwnd),
                    &format!(
                        "{} is already taken by another program{}.",
                        chosen.label(),
                        if chosen == Hotkey::PrintScreen {
                            " (Windows 11 gives Print Screen to Snipping Tool unless you turn that off in Settings → Accessibility → Keyboard)"
                        } else {
                            ""
                        }
                    ),
                    MB_ICONWARNING,
                );
                settings::update(|s| s.hotkey = before);
            }
            let ok = ok || app::register_hotkey();
            unsafe {
                SendMessageW(hwnd, WM_USER, Some(WPARAM(ok as usize)), None);
            }
            update_tray_tip(hwnd);
        }
        c if (CMD_SNIP..CMD_SNIP + 3).contains(&c) => app::start(Mode::ALL[c - CMD_SNIP], 0),
        CMD_SHOW => app::show_launcher(),
        CMD_EXIT => {
            remove_tray(hwnd);
            unsafe { PostQuitMessage(0) };
        }
        _ => {}
    }
    ui::invalidate(hwnd);
}

fn tray_menu(hwnd: HWND) {
    let mut items: Vec<(usize, String, bool)> = Mode::ALL
        .iter()
        .enumerate()
        .map(|(i, m)| (CMD_SNIP + i, format!("{} snip", m.label()), false))
        .collect();
    items.push((0, String::new(), false));
    items.push((CMD_SHOW, "Show Snapleaf".into(), false));
    items.extend(hotkey_items());
    items.push((0, String::new(), false));
    items.push((CMD_EXIT, "Exit".into(), false));
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    let cmd = ui::popup_menu(hwnd, (p.x, p.y), &items);
    run_command(hwnd, cmd);
}

impl Launcher {
    fn buttons(&self) -> Vec<Button<Btn>> {
        let s = self.scale;
        let set = settings::get();
        let fmt = text_format(14.0 * s, DWRITE_FONT_WEIGHT_NORMAL).ok();
        let mut v = vec![
            Button::new(Btn::New, Icon::New, "New snip").label("New").look(Look::Accent),
            Button::new(Btn::Mode, mode_icon(set.mode), "Snipping mode").label(set.mode.label()).menu(),
            Button::new(Btn::Delay, Icon::Delay, "Delay before snipping").label(&delay_label(set.delay)).menu(),
            Button::new(Btn::More, Icon::More, "More"),
        ];
        let h = 40.0 * s;
        let mut x = 12.0 * s;
        let y = 12.0 * s;
        let count = v.len();
        for (i, b) in v.iter_mut().enumerate() {
            let w = fmt.as_ref().map(|f| b.width(h, s, f)).unwrap_or(h);
            if i == count - 1 {
                b.rect = Rect::xywh(CLIENT_W * s - 12.0 * s - h, y, h, h);
            } else {
                b.rect = Rect::xywh(x, y, w, h);
                x += w + 6.0 * s;
            }
        }
        v
    }

    fn hit(&self, p: Pt) -> Option<Btn> {
        self.buttons().into_iter().find(|b| b.rect.contains(p)).map(|b| b.id)
    }

    fn open_menu(&mut self, hwnd: HWND, b: Btn) {
        let Some(btn) = self.buttons().into_iter().find(|x| x.id == b) else { return };
        let set = settings::get();
        let items: Vec<(usize, String, bool)> = match b {
            Btn::Mode => Mode::ALL.iter().enumerate().map(|(i, m)| (CMD_MODE + i, m.label().into(), *m == set.mode)).collect(),
            Btn::Delay => {
                DELAYS.iter().enumerate().map(|(i, d)| (CMD_DELAY + i, delay_label(*d), *d == set.delay)).collect()
            }
            Btn::More => {
                let mut v = hotkey_items();
                v.push((0, String::new(), false));
                v.push((CMD_EXIT, "Exit Snapleaf".into(), false));
                v
            }
            Btn::New => return,
        };
        let at = ui::to_screen(hwnd, pt(btn.rect.l, btn.rect.b + 2.0 * self.scale));
        self.pressed = None;
        let cmd = ui::popup_menu(hwnd, at, &items);
        run_command(hwnd, cmd);
    }

    fn paint(&mut self, hwnd: HWND, rt: &windows::Win32::Graphics::Direct2D::ID2D1RenderTarget) -> Result<()> {
        let s = self.scale;
        let t = ui::theme();
        let c = Canvas::new(rt)?;
        c.clear(t.panel);
        let fmt = text_format(14.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        for b in self.buttons() {
            let st = ButtonState {
                hover: self.hover == Some(b.id),
                pressed: self.pressed == Some(b.id),
                active: false,
                enabled: true,
            };
            ui::draw_button(&c, &t, &b, st, s, &fmt);
        }
        let hint = text_format(12.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        let msg = if self.hotkey_ok {
            format!("Press {} from anywhere. Close this window to keep Snapleaf in the tray.", hotkey_hint().trim_end_matches(" to snip"))
        } else {
            format!("{} is used by another app — pick another shortcut under ⋯", settings::get().hotkey.label())
        };
        let msg = if settings::get().hotkey == Hotkey::Off {
            "Close this window to keep Snapleaf in the tray.".to_string()
        } else {
            msg
        };
        let r = ui::client_rect(hwnd);
        c.text(&msg, &hint, Rect::new(14.0 * s, 62.0 * s, r.r - 12.0 * s, r.b), if self.hotkey_ok { t.dim } else { Color(0xC42B1C) }, 1.0);
        Ok(())
    }
}

impl Handler for Launcher {
    fn on(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if msg == self.taskbar_created && msg != 0 {
            add_tray(hwnd);
            return Some(LRESULT(0));
        }
        match msg {
            WM_USER => {
                self.hotkey_ok = wp.0 != 0;
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                let mut target = std::mem::take(&mut self.target);
                target.paint(hwnd, |rt, _| self.paint(hwnd, rt));
                self.target = target;
                unsafe {
                    let _ = EndPaint(hwnd, &ps);
                }
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_SIZE => {
                self.target.resize((lp.0 & 0xFFFF) as u32, ((lp.0 >> 16) & 0xFFFF) as u32);
                None
            }
            WM_DPICHANGED => {
                self.scale = (wp.0 & 0xFFFF) as f32 / 96.0;
                let r = unsafe { &*(lp.0 as *const RECT) };
                let (w, h) = window_size(self.scale, (wp.0 & 0xFFFF) as u32);
                unsafe {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE => {
                if ui::refresh_theme() {
                    ui::style_frame(hwnd, ui::theme().dark);
                    ui::invalidate(hwnd);
                }
                None
            }
            WM_MOUSEMOVE => {
                let h = self.hit(ui::xy(lp));
                if h != self.hover {
                    self.hover = h;
                    ui::track_leave(hwnd);
                    ui::invalidate(hwnd);
                }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.hover = None;
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let b = self.hit(ui::xy(lp));
                self.pressed = b;
                ui::invalidate(hwnd);
                if let Some(b @ (Btn::Mode | Btn::Delay | Btn::More)) = b {
                    self.open_menu(hwnd, b);
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let b = self.hit(ui::xy(lp));
                let was = self.pressed.take();
                ui::invalidate(hwnd);
                if b == Some(Btn::New) && was == Some(Btn::New) {
                    app::start_saved();
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let ctrl = ui::key_down(0x11);
                match wp.0 as u8 {
                    b'N' if ctrl => app::start_saved(),
                    0x0D => app::start_saved(),
                    0x1B => unsafe {
                        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    },
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                if !self.told_about_tray {
                    self.told_about_tray = true;
                    tray_balloon(hwnd, &format!("{}, or use the tray icon.", hotkey_hint()));
                }
                Some(LRESULT(0))
            }
            WM_HOTKEY => {
                if wp.0 as i32 == app::HOTKEY_ID {
                    app::start_from_hotkey();
                }
                Some(LRESULT(0))
            }
            WM_TIMER => {
                app::on_timer(wp.0);
                Some(LRESULT(0))
            }
            m if m == app::WM_APP_TRAY => {
                match (lp.0 & 0xFFFF) as u32 {
                    WM_LBUTTONUP => app::show_launcher(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(hwnd),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            m if m == app::WM_APP_CAPTURED => {
                let img = unsafe { *Box::from_raw(lp.0 as *mut Option<Image>) };
                app::finish(img);
                Some(LRESULT(0))
            }
            m if m == app::WM_APP_START => {
                if !app::is_busy() {
                    match wp.0 {
                        1..=3 => app::start(Mode::ALL[wp.0 - 1], lp.0 as u32),
                        _ => app::start_saved(),
                    }
                }
                Some(LRESULT(0))
            }
            m if m == app::WM_APP_SHOW => {
                app::show_launcher();
                Some(LRESULT(0))
            }
            m if m == app::WM_APP_CANCEL => {
                app::cancel();
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                remove_tray(hwnd);
                unsafe { PostQuitMessage(0) };
                None
            }
            _ => None,
        }
    }
}
