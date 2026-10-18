//! Window plumbing and the small custom-drawn widget kit shared by the
//! launcher, the capture overlay and the editor: theme, vector icons, buttons.

use std::cell::Cell;
use std::ffi::c_void;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::geom::*;
use crate::gfx::*;

// ---------------------------------------------------------------- windows

/// Per-window state lives in a `Box<T>` hung off `GWLP_USERDATA`.
pub trait Handler {
    /// `None` falls through to `DefWindowProcW`.
    fn on(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT>;
}

unsafe extern "system" fn proc_for<T: Handler>(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }
        let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut T;
        if p.is_null() {
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(p));
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        match (*p).on(hwnd, msg, wp, lp) {
            Some(r) => r,
            None => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

pub fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).unwrap_or_default() }
}

pub struct WindowSpec<'a> {
    pub class: PCWSTR,
    pub title: &'a str,
    pub style: WINDOW_STYLE,
    pub ex: WINDOW_EX_STYLE,
    pub rect: (i32, i32, i32, i32),
    pub cursor: PCWSTR,
    pub owner: Option<HWND>,
}

pub fn create_window<T: Handler>(spec: WindowSpec, state: T) -> Result<HWND> {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(proc_for::<T>),
            hInstance: hinstance(),
            hCursor: LoadCursorW(None, spec.cursor).unwrap_or_default(),
            hIcon: app_icon(large_icon_size()),
            hIconSm: app_icon(small_icon_size()),
            lpszClassName: spec.class,
            ..Default::default()
        };
        // Fails harmlessly once the class exists.
        RegisterClassExW(&wc);
        let p = Box::into_raw(Box::new(state));
        let (x, y, w, h) = spec.rect;
        CreateWindowExW(
            spec.ex,
            spec.class,
            &HSTRING::from(spec.title),
            spec.style,
            x,
            y,
            w,
            h,
            spec.owner,
            None,
            Some(hinstance()),
            Some(p as *const c_void),
        )
    }
}

pub fn xy(lp: LPARAM) -> Pt {
    pt((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32)
}

pub fn client_rect(hwnd: HWND) -> Rect {
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    Rect::new(0.0, 0.0, rc.right as f32, rc.bottom as f32)
}

pub fn dpi_scale(hwnd: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

pub fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}

pub fn cursor_pos(hwnd: HWND) -> Pt {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
        let _ = ScreenToClient(hwnd, &mut p);
    }
    pt(p.x as f32, p.y as f32)
}

/// Ask for a `WM_MOUSELEAVE` so hover highlights can be cleared.
pub fn track_leave(hwnd: HWND) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
    let mut t = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe {
        let _ = TrackMouseEvent(&mut t);
    }
}

pub fn to_screen(hwnd: HWND, p: Pt) -> (i32, i32) {
    let mut q = POINT { x: p.x as i32, y: p.y as i32 };
    unsafe {
        let _ = ClientToScreen(hwnd, &mut q);
    }
    (q.x, q.y)
}

/// A popup menu: `(id, label, checked)`; id 0 is a separator. Returns the
/// chosen id, or 0.
pub fn popup_menu(hwnd: HWND, at: (i32, i32), items: &[(usize, String, bool)]) -> usize {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let mut sub: Option<HMENU> = None;
        let mut sub_label = String::new();
        for (id, label, checked) in items {
            // Items whose label starts with "»" go into a submenu titled by the
            // preceding "»»" item.
            if let Some(title) = label.strip_prefix("»»") {
                if let Some(s) = sub.take() {
                    let _ = AppendMenuW(menu, MF_POPUP, s.0 as usize, &HSTRING::from(sub_label.as_str()));
                }
                sub = CreatePopupMenu().ok();
                sub_label = title.to_string();
                continue;
            }
            let target = match (label.strip_prefix('»'), sub) {
                (Some(_), Some(s)) => s,
                _ => {
                    if let Some(s) = sub.take() {
                        let _ = AppendMenuW(menu, MF_POPUP, s.0 as usize, &HSTRING::from(sub_label.as_str()));
                    }
                    menu
                }
            };
            let text = label.trim_start_matches('»');
            if *id == 0 {
                let _ = AppendMenuW(target, MF_SEPARATOR, 0, PCWSTR::null());
            } else {
                let flags = if *checked { MF_STRING | MF_CHECKED } else { MF_STRING };
                let _ = AppendMenuW(target, flags, *id, &HSTRING::from(text));
            }
        }
        if let Some(s) = sub.take() {
            let _ = AppendMenuW(menu, MF_POPUP, s.0 as usize, &HSTRING::from(sub_label.as_str()));
        }
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN, at.0, at.1, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0 as usize
    }
}

pub fn set_cursor(id: PCWSTR) {
    unsafe {
        if let Ok(c) = LoadCursorW(None, id) {
            SetCursor(Some(c));
        }
    }
}

/// Dark title bar and Windows 11 rounded corners where available.
pub fn style_frame(hwnd: HWND, dark: bool) {
    unsafe {
        let v: BOOL = dark.into();
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &v as *const _ as *const c_void, 4);
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as *const c_void, 4);
    }
}

/// Work area (screen pixels) and DPI scale of the monitor under the cursor.
pub fn cursor_monitor() -> (Rect, f32) {
    unsafe {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = windows::Win32::UI::HiDpi::GetDpiForMonitor(
            mon,
            windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI,
            &mut dx,
            &mut dy,
        );
        let w = mi.rcWork;
        (
            Rect::new(w.left as f32, w.top as f32, w.right as f32, w.bottom as f32),
            dx.max(96) as f32 / 96.0,
        )
    }
}

pub fn message_box(owner: Option<HWND>, text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe { MessageBoxW(owner, &HSTRING::from(text), w!("Snapleaf"), flags) }
}

// ---------------------------------------------------------------- theme

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub panel: Color,
    pub canvas: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub on_accent: Color,
}

const LIGHT: Theme = Theme {
    dark: false,
    panel: Color(0xF3F3F3),
    canvas: Color(0xE4E4E4),
    text: Color(0x1B1B1B),
    dim: Color(0x616161),
    accent: Color(0x005FB8),
    on_accent: Color(0xFFFFFF),
};

const DARK: Theme = Theme {
    dark: true,
    panel: Color(0x202020),
    canvas: Color(0x141414),
    text: Color(0xFFFFFF),
    dim: Color(0xC5C5C5),
    accent: Color(0x60CDFF),
    on_accent: Color(0x000000),
};

thread_local! {
    static DARK_MODE: Cell<Option<bool>> = const { Cell::new(None) };
}

pub fn theme() -> Theme {
    let dark = DARK_MODE.with(|d| {
        if d.get().is_none() {
            d.set(Some(read_dark_mode()));
        }
        d.get().unwrap_or(false)
    });
    if dark { DARK } else { LIGHT }
}

/// Call on `WM_SETTINGCHANGE`; returns true when the theme flipped.
pub fn refresh_theme() -> bool {
    let now = read_dark_mode();
    DARK_MODE.with(|d| d.replace(Some(now)) != Some(now))
}

fn read_dark_mode() -> bool {
    let mut v = 1u32;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut _ as *mut c_void),
            Some(&mut size),
        )
    };
    r.is_ok() && v == 0
}

// ---------------------------------------------------------------- icons

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    None,
    New,
    Select,
    Rect,
    Arrow,
    Text,
    Highlight,
    Pen,
    Crop,
    Undo,
    Redo,
    Copy,
    Save,
    Chevron,
    ModeRegion,
    ModeWindow,
    ModeFull,
    Delay,
    Close,
    More,
}

/// Draw a 20×20-unit line icon centred on `ctr`, `size` pixels across.
pub fn draw_icon(c: &Canvas, icon: Icon, ctr: Pt, size: f32, col: Color, a: f32) {
    use Seg::*;
    let s = size / 20.0;
    let o = pt(ctr.x - size / 2.0, ctr.y - size / 2.0);
    let w = 1.5 * s;
    let stroke = |segs: &[Seg]| {
        if let Ok(g) = path(segs, o, s, false) {
            c.stroke_geom(&g, col, a, w);
        }
    };
    let fill = |segs: &[Seg], alpha: f32| {
        if let Ok(g) = path(segs, o, s, true) {
            c.fill_geom(&g, col, alpha);
        }
    };
    match icon {
        Icon::None => {}
        Icon::New => stroke(&[M(10.0, 4.0), L(10.0, 16.0), M(4.0, 10.0), L(16.0, 10.0)]),
        Icon::Select => stroke(&[
            M(6.0, 3.0),
            L(6.0, 16.0),
            L(9.2, 12.9),
            L(11.6, 17.6),
            L(13.6, 16.6),
            L(11.3, 12.0),
            L(15.5, 12.0),
            Z,
        ]),
        Icon::Rect => stroke(&[M(3.5, 5.5), L(16.5, 5.5), L(16.5, 14.5), L(3.5, 14.5), Z]),
        Icon::Arrow => {
            stroke(&[M(4.0, 16.0), L(13.0, 7.0)]);
            fill(&[M(16.5, 3.5), L(9.0, 5.5), L(14.5, 11.0), Z], a);
        }
        Icon::Text => stroke(&[M(4.5, 4.5), L(15.5, 4.5), M(10.0, 4.5), L(10.0, 16.0), M(7.5, 16.0), L(12.5, 16.0)]),
        Icon::Highlight => {
            fill(&[M(3.0, 5.5), L(17.0, 5.5), L(17.0, 12.5), L(3.0, 12.5), Z], a * 0.4);
            stroke(&[M(3.0, 16.0), L(17.0, 16.0)]);
        }
        Icon::Pen => stroke(&[M(3.0, 14.0), C(6.0, 5.0, 9.0, 5.0, 10.0, 10.0), C(11.0, 15.0, 14.0, 15.0, 17.0, 6.0)]),
        Icon::Crop => stroke(&[M(6.0, 2.5), L(6.0, 14.0), L(17.5, 14.0), M(2.5, 6.0), L(14.0, 6.0), L(14.0, 17.5)]),
        Icon::Undo => stroke(&[
            M(8.0, 4.0),
            L(4.0, 8.0),
            L(8.0, 12.0),
            M(4.5, 8.0),
            L(12.0, 8.0),
            A(12.0, 16.0, 4.0, true),
            L(8.0, 16.0),
        ]),
        Icon::Redo => stroke(&[
            M(12.0, 4.0),
            L(16.0, 8.0),
            L(12.0, 12.0),
            M(15.5, 8.0),
            L(8.0, 8.0),
            A(8.0, 16.0, 4.0, false),
            L(12.0, 16.0),
        ]),
        Icon::Copy => stroke(&[
            M(7.0, 7.0),
            L(7.0, 3.5),
            L(16.5, 3.5),
            L(16.5, 13.0),
            L(13.0, 13.0),
            M(3.5, 7.0),
            L(13.0, 7.0),
            L(13.0, 16.5),
            L(3.5, 16.5),
            Z,
        ]),
        Icon::Save => stroke(&[
            M(10.0, 3.0),
            L(10.0, 12.5),
            M(6.0, 8.5),
            L(10.0, 12.5),
            L(14.0, 8.5),
            M(3.5, 12.5),
            L(3.5, 16.5),
            L(16.5, 16.5),
            L(16.5, 12.5),
        ]),
        Icon::Chevron => stroke(&[M(6.0, 8.0), L(10.0, 12.0), L(14.0, 8.0)]),
        Icon::ModeRegion => stroke(&[
            M(3.0, 7.0),
            L(3.0, 4.0),
            L(6.0, 4.0),
            M(14.0, 4.0),
            L(17.0, 4.0),
            L(17.0, 7.0),
            M(17.0, 13.0),
            L(17.0, 16.0),
            L(14.0, 16.0),
            M(6.0, 16.0),
            L(3.0, 16.0),
            L(3.0, 13.0),
        ]),
        Icon::ModeWindow => stroke(&[M(3.0, 4.5), L(17.0, 4.5), L(17.0, 15.5), L(3.0, 15.5), Z, M(3.0, 7.5), L(17.0, 7.5)]),
        Icon::ModeFull => stroke(&[
            M(2.5, 4.0),
            L(17.5, 4.0),
            L(17.5, 13.5),
            L(2.5, 13.5),
            Z,
            M(10.0, 13.5),
            L(10.0, 16.5),
            M(6.5, 16.5),
            L(13.5, 16.5),
        ]),
        Icon::Delay => stroke(&[
            M(10.0, 4.5),
            A(10.0, 17.5, 6.5, true),
            A(10.0, 4.5, 6.5, true),
            M(10.0, 7.5),
            L(10.0, 11.0),
            L(12.5, 12.5),
            M(8.5, 2.0),
            L(11.5, 2.0),
        ]),
        Icon::Close => stroke(&[M(5.0, 5.0), L(15.0, 15.0), M(15.0, 5.0), L(5.0, 15.0)]),
        Icon::More => {
            for x in [4.5, 10.0, 15.5] {
                c.fill_circle(pt(o.x + x * s, o.y + 10.0 * s), 1.4 * s, col, a);
            }
        }
    }
}

// ---------------------------------------------------------------- buttons

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Look {
    Plain,
    Accent,
    Swatch(Color),
    /// A filled dot of the given radius (in 20-unit icon space): stroke sizes.
    Dot(f32),
}

#[derive(Clone)]
pub struct Button<I: Copy + PartialEq> {
    pub id: I,
    pub rect: Rect,
    pub icon: Icon,
    pub label: String,
    pub tip: String,
    pub look: Look,
    /// Trailing ▾ for buttons that open a menu.
    pub menu: bool,
}

impl<I: Copy + PartialEq> Button<I> {
    pub fn new(id: I, icon: Icon, tip: &str) -> Self {
        Button { id, rect: Rect::default(), icon, label: String::new(), tip: tip.into(), look: Look::Plain, menu: false }
    }
    pub fn label(mut self, s: &str) -> Self {
        self.label = s.into();
        self
    }
    pub fn look(mut self, l: Look) -> Self {
        self.look = l;
        self
    }
    pub fn menu(mut self) -> Self {
        self.menu = true;
        self
    }
    /// Natural width for a toolbar of height-`h` buttons.
    pub fn width(&self, h: f32, scale: f32, fmt: &IDWriteTextFormat) -> f32 {
        let mut w = if self.icon == Icon::None && !self.label.is_empty() { 12.0 * scale } else { h };
        if !self.label.is_empty() {
            if let Ok(l) = text_layout(&self.label, fmt) {
                w += layout_size(&l).0 + if self.icon == Icon::None { 12.0 * scale } else { 2.0 * scale };
            }
            if self.icon != Icon::None {
                w += 10.0 * scale;
            }
        }
        if self.menu {
            w += 14.0 * scale;
        }
        w
    }
}

pub struct ButtonState {
    pub hover: bool,
    pub pressed: bool,
    pub active: bool,
    pub enabled: bool,
}

pub fn draw_button<I: Copy + PartialEq>(
    c: &Canvas,
    t: &Theme,
    b: &Button<I>,
    st: ButtonState,
    scale: f32,
    fmt: &IDWriteTextFormat,
) {
    let r = b.rect.inflate(-2.0 * scale);
    let rad = 4.0 * scale;
    let alpha = if st.enabled { 1.0 } else { 0.35 };
    let mut fg = t.text;
    match b.look {
        Look::Accent => {
            c.fill_round(r, rad, t.accent, if st.pressed { 0.8 } else if st.hover { 0.9 } else { 1.0 });
            fg = t.on_accent;
        }
        _ => {
            if st.enabled && (st.hover || st.pressed || st.active) {
                let k = if st.pressed { 0.10 } else if st.active { 0.08 } else { 0.06 };
                c.fill_round(r, rad, t.text, k);
            }
        }
    }
    let h = b.rect.h();
    let icon_size = 20.0 * scale;
    match b.look {
        Look::Swatch(col) => {
            let ctr = b.rect.center();
            let rr = 8.0 * scale;
            c.fill_circle(ctr, rr, col, alpha);
            c.stroke_circle(ctr, rr, t.text, 0.25 * alpha, 1.0);
            if st.active {
                c.stroke_circle(ctr, rr + 3.0 * scale, t.text, 0.9, 1.5 * scale);
            }
        }
        Look::Dot(rad_units) => {
            c.fill_circle(b.rect.center(), rad_units * scale, fg, alpha);
        }
        _ => {
            let mut x = b.rect.l;
            if b.icon != Icon::None {
                draw_icon(c, b.icon, pt(x + h / 2.0, b.rect.center().y), icon_size, fg, alpha);
                x += h - 4.0 * scale;
            } else {
                x += 12.0 * scale;
            }
            if !b.label.is_empty() {
                let tr = Rect::new(x, b.rect.t, b.rect.r - if b.menu { 18.0 * scale } else { 0.0 }, b.rect.b);
                if let Ok(l) = text_layout(&b.label, fmt) {
                    let (_, th) = layout_size(&l);
                    c.layout(&l, pt(tr.l, (tr.t + tr.b - th) / 2.0), fg, alpha);
                }
            }
            if b.menu {
                draw_icon(c, Icon::Chevron, pt(b.rect.r - 13.0 * scale, b.rect.center().y), 14.0 * scale, fg, alpha);
            }
        }
    }
    if st.active && matches!(b.look, Look::Plain | Look::Dot(_)) {
        let bar = Rect::xywh(b.rect.center().x - 8.0 * scale, r.b - 3.0 * scale, 16.0 * scale, 3.0 * scale);
        c.fill_round(bar, 1.5 * scale, t.accent, 1.0);
    }
}

/// A small tooltip below (or above, if no room) `anchor`, kept inside `bounds`.
pub fn draw_tip(c: &Canvas, t: &Theme, text: &str, anchor: Rect, bounds: Rect, scale: f32, fmt: &IDWriteTextFormat) {
    let Ok(l) = text_layout(text, fmt) else { return };
    let (tw, th) = layout_size(&l);
    let (pw, ph) = (tw + 16.0 * scale, th + 10.0 * scale);
    let mut x = (anchor.center().x - pw / 2.0).clamp(bounds.l + 4.0, (bounds.r - pw - 4.0).max(bounds.l));
    let mut y = anchor.b + 6.0 * scale;
    if y + ph > bounds.b {
        y = anchor.t - ph - 6.0 * scale;
    }
    x = x.round();
    y = y.round();
    let r = Rect::xywh(x, y, pw, ph);
    let bg = if t.dark { Color(0x2C2C2C) } else { Color(0xFFFFFF) };
    c.fill_round(r, 4.0 * scale, bg, 0.98);
    c.stroke_round(r, 4.0 * scale, t.text, 0.15, 1.0);
    c.layout(&l, pt(x + 8.0 * scale, y + 5.0 * scale), t.text, 1.0);
}

// ---------------------------------------------------------------- app icon

thread_local! {
    static ICONS: std::cell::RefCell<Vec<(i32, HICON)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The Snapleaf leaf and capture-frame icon, loaded at the system DPI size.
pub fn app_icon(size: i32) -> HICON {
    if let Some(icon) = ICONS.with(|c| c.borrow().iter().find(|(s, _)| *s == size).map(|(_, i)| *i)) {
        return icon;
    }
    let icon = make_icon(size as u32).unwrap_or_default();
    ICONS.with(|c| c.borrow_mut().push((size, icon)));
    icon
}

/// System icon sizes for the primary display, where the taskbar and tray
/// live; a 16 px icon stretched to 24 px at 150% looks blurry.
pub fn small_icon_size() -> i32 {
    unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) }
}

pub fn large_icon_size() -> i32 {
    unsafe { GetSystemMetricsForDpi(SM_CXICON, GetDpiForSystem()) }
}

fn make_icon(n: u32) -> Result<HICON> {
    // Resource 1 is also the Explorer icon. Windows picks the closest embedded
    // frame, keeping tray, title bar and executable artwork in sync.
    unsafe {
        LoadImageW(Some(hinstance()), PCWSTR(std::ptr::without_provenance(1)), IMAGE_ICON, n as i32, n as i32, LR_DEFAULTCOLOR)
            .map(|handle| HICON(handle.0))
    }
}
