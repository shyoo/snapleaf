//! The selection overlay: the frozen desktop under a dim veil, a mode bar at
//! the top, and either a drag-to-select rectangle or a window highlight.

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::app;
use crate::capture::Frozen;
use crate::geom::*;
use crate::gfx::*;
use crate::settings::{self, Mode};
use crate::ui::{self, Button, ButtonState, Handler, Icon, WindowSpec};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Bar {
    Mode(Mode),
    Close,
}

struct Overlay {
    frozen: Frozen,
    mode: Mode,
    target: WindowTarget,
    bmp: Option<ID2D1Bitmap>,
    drag: Option<Pt>,
    cur: Pt,
    hover_win: Option<usize>,
    bar: Vec<Button<Bar>>,
    bar_rect: Rect,
    hover_btn: Option<Bar>,
    pressed: Option<Bar>,
    scale: f32,
    done: bool,
}

const VEIL: f32 = 0.45;
const MIN_DRAG: f32 = 3.0;

pub fn show(frozen: Frozen, mode: Mode) -> Result<HWND> {
    let (work, scale) = ui::cursor_monitor();
    let (w, h) = (frozen.img.w as i32, frozen.img.h as i32);
    let origin = pt(frozen.x as f32, frozen.y as f32);

    let mut bar = vec![
        Button::new(Bar::Mode(Mode::Region), Icon::ModeRegion, "Rectangle"),
        Button::new(Bar::Mode(Mode::Window), Icon::ModeWindow, "Window"),
        Button::new(Bar::Mode(Mode::Full), Icon::ModeFull, "Full screen"),
        Button::new(Bar::Close, Icon::Close, "Cancel (Esc)"),
    ];
    let bs = 40.0 * scale;
    let pad = 4.0 * scale;
    let gap = 8.0 * scale;
    let bw = bs * 4.0 + gap + pad * 2.0;
    let left = (work.center().x - origin.x - bw / 2.0).round();
    let top = (work.t - origin.y + 12.0 * scale).round();
    let bar_rect = Rect::xywh(left, top, bw, bs + pad * 2.0);
    let mut x = left + pad;
    for (i, b) in bar.iter_mut().enumerate() {
        if i == 3 {
            x += gap;
        }
        b.rect = Rect::xywh(x, top + pad, bs, bs);
        x += bs;
    }

    let state = Overlay {
        frozen,
        mode,
        target: WindowTarget::default(),
        bmp: None,
        drag: None,
        cur: pt(-100.0, -100.0),
        hover_win: None,
        bar,
        bar_rect,
        hover_btn: None,
        pressed: None,
        scale,
        done: false,
    };
    let hwnd = ui::create_window(
        WindowSpec {
            class: w!("SnipperOverlay"),
            title: "Snipper",
            style: WS_POPUP,
            ex: WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            rect: (origin.x as i32, origin.y as i32, w, h),
            cursor: IDC_CROSS,
            owner: None,
        },
        state,
    )?;
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
    Ok(hwnd)
}

impl Overlay {
    fn selection(&self) -> Option<Rect> {
        match self.mode {
            Mode::Region => self.drag.map(|s| Rect::from_pts(s, self.cur).round()),
            Mode::Window => self.hover_win.map(|i| self.frozen.windows[i]),
            Mode::Full => None,
        }
    }

    fn button_at(&self, p: Pt) -> Option<Bar> {
        self.bar.iter().find(|b| b.rect.contains(p)).map(|b| b.id)
    }

    fn finish(&mut self, hwnd: HWND, r: Option<Rect>) {
        if self.done {
            return;
        }
        self.done = true;
        let img = r.map(|r| {
            let r = r.round();
            self.frozen.img.sub(r.l as i32, r.t as i32, r.w() as i32, r.h() as i32)
        });
        unsafe {
            let _ = ReleaseCapture();
            let _ = ShowWindow(hwnd, SW_HIDE);
            let payload = Box::into_raw(Box::new(img));
            let _ = PostMessageW(Some(app::host()), app::WM_APP_CAPTURED, WPARAM(0), LPARAM(payload as isize));
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    fn full(&self) -> Rect {
        Rect::new(0.0, 0.0, self.frozen.img.w as f32, self.frozen.img.h as f32)
    }

    fn press_bar(&mut self, hwnd: HWND, b: Bar) {
        match b {
            Bar::Mode(Mode::Full) => {
                settings::update(|s| s.mode = Mode::Full);
                let full = self.full();
                self.finish(hwnd, Some(full));
            }
            Bar::Mode(m) => {
                self.mode = m;
                self.drag = None;
                settings::update(|s| s.mode = m);
                self.update_hover();
                ui::invalidate(hwnd);
            }
            Bar::Close => self.finish(hwnd, None),
        }
    }

    fn update_hover(&mut self) {
        self.hover_win = if self.mode == Mode::Window && !self.bar_rect.contains(self.cur) {
            self.frozen.windows.iter().position(|r| r.contains(self.cur))
        } else {
            None
        };
    }

    fn paint(&mut self, rt: &ID2D1RenderTarget, fresh: bool) -> Result<()> {
        if fresh || self.bmp.is_none() {
            self.bmp = Some(self.frozen.img.to_bitmap(rt)?);
        }
        let c = Canvas::new(rt)?;
        let s = self.scale;
        let full = self.full();
        if let Some(bmp) = &self.bmp {
            unsafe { rt.DrawBitmap(bmp, Some(&full.d2d()), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None) };
        }
        let accent = Color(0x60CDFF);
        match self.selection().filter(|r| !r.is_empty()) {
            Some(r) => {
                c.fill(Rect::new(full.l, full.t, full.r, r.t), Color::BLACK, VEIL);
                c.fill(Rect::new(full.l, r.b, full.r, full.b), Color::BLACK, VEIL);
                c.fill(Rect::new(full.l, r.t, r.l, r.b), Color::BLACK, VEIL);
                c.fill(Rect::new(r.r, r.t, full.r, r.b), Color::BLACK, VEIL);
                if self.mode == Mode::Window {
                    c.stroke(r.inflate(-1.5 * s), accent, 1.0, 3.0 * s);
                } else {
                    c.stroke(r.inflate(0.5), accent, 1.0, 1.0);
                }
                self.size_label(&c, r)?;
            }
            None => c.fill(full, Color::BLACK, VEIL),
        }
        if self.mode == Mode::Region && !self.bar_rect.contains(self.cur) {
            if self.drag.is_none() {
                let p = pt(self.cur.x.floor() + 0.5, self.cur.y.floor() + 0.5);
                c.line(pt(full.l, p.y), pt(full.r, p.y), Color::WHITE, 0.35, 1.0);
                c.line(pt(p.x, full.t), pt(p.x, full.b), Color::WHITE, 0.35, 1.0);
            }
            self.loupe(&c, rt)?;
        }
        self.paint_bar(&c)
    }

    fn size_label(&self, c: &Canvas, r: Rect) -> Result<()> {
        let s = self.scale;
        let f = centered_format(12.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
        let text = format!("{} × {}", r.w() as i32, r.h() as i32);
        let l = text_layout(&text, &f)?;
        let (tw, th) = layout_size(&l);
        let (pw, ph) = (tw + 14.0 * s, th + 8.0 * s);
        let mut y = r.b + 6.0 * s;
        if y + ph > self.full().b - 4.0 {
            y = r.t - ph - 6.0 * s;
        }
        if y < 0.0 {
            y = r.t + 6.0 * s;
        }
        let x = r.l.max(4.0).min(self.full().r - pw - 4.0);
        let pill = Rect::xywh(x.round(), y.round(), pw, ph);
        c.fill_round(pill, 4.0 * s, Color(0x1F1F1F), 0.9);
        c.text(&text, &f, pill, Color::WHITE, 1.0);
        Ok(())
    }

    /// Magnified pixels around the cursor, for precise edges.
    fn loupe(&self, c: &Canvas, rt: &ID2D1RenderTarget) -> Result<()> {
        let Some(bmp) = &self.bmp else { return Ok(()) };
        let s = self.scale;
        let cells = 15.0;
        let zoom = (8.0 * s).round();
        let size = cells * zoom;
        let full = self.full();
        let mut x = self.cur.x + 20.0 * s;
        let mut y = self.cur.y + 20.0 * s;
        if x + size > full.r {
            x = self.cur.x - 20.0 * s - size;
        }
        if y + size + 26.0 * s > full.b {
            y = self.cur.y - 20.0 * s - size - 26.0 * s;
        }
        let dst = Rect::xywh(x.round(), y.round(), size, size);
        let cx = self.cur.x.floor();
        let cy = self.cur.y.floor();
        let src = Rect::new(cx - 7.0, cy - 7.0, cx + 8.0, cy + 8.0);
        c.fill(dst.inflate(1.0), Color::BLACK, 1.0);
        unsafe {
            rt.DrawBitmap(
                bmp,
                Some(&dst.d2d()),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                Some(&src.d2d()),
            )
        };
        let mid = Rect::xywh(dst.l + 7.0 * zoom, dst.t + 7.0 * zoom, zoom, zoom);
        c.stroke(mid, Color::WHITE, 1.0, 1.0);
        c.stroke(mid.inflate(1.0), Color::BLACK, 0.6, 1.0);
        c.stroke(dst.inflate(1.0), Color::WHITE, 0.8, 1.0);
        let f = centered_format(11.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        let info = Rect::xywh(dst.l, dst.b + 4.0 * s, size, 20.0 * s);
        c.fill_round(info, 3.0 * s, Color(0x1F1F1F), 0.9);
        c.text(&format!("{}, {}", cx as i32, cy as i32), &f, info, Color::WHITE, 1.0);
        Ok(())
    }

    fn paint_bar(&self, c: &Canvas) -> Result<()> {
        let s = self.scale;
        let t = ui::theme();
        c.fill_round(self.bar_rect, 8.0 * s, t.panel, 0.98);
        c.stroke_round(self.bar_rect, 8.0 * s, t.text, 0.12, 1.0);
        let fmt = text_format(12.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        for b in &self.bar {
            let st = ButtonState {
                hover: self.hover_btn == Some(b.id),
                pressed: self.pressed == Some(b.id) && self.hover_btn == Some(b.id),
                active: b.id == Bar::Mode(self.mode),
                enabled: true,
            };
            ui::draw_button(c, &t, b, st, s, &fmt);
        }
        let hint = match (self.hover_btn, self.mode) {
            (Some(id), _) => self.bar.iter().find(|b| b.id == id).map(|b| b.tip.clone()).unwrap_or_default(),
            (None, Mode::Window) => "Click a window to capture it  ·  Esc to cancel".into(),
            (None, _) => "Drag to select an area  ·  Esc to cancel".into(),
        };
        let hf = centered_format(12.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
        let l = text_layout(&hint, &hf)?;
        let (tw, th) = layout_size(&l);
        let r = Rect::xywh(
            (self.bar_rect.center().x - tw / 2.0 - 10.0 * s).round(),
            self.bar_rect.b + 8.0 * s,
            tw + 20.0 * s,
            th + 8.0 * s,
        );
        c.fill_round(r, 4.0 * s, Color(0x1F1F1F), 0.85);
        c.text(&hint, &hf, r, Color::WHITE, 1.0);
        Ok(())
    }
}

impl Handler for Overlay {
    fn on(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                let mut target = std::mem::take(&mut self.target);
                target.paint(hwnd, |rt, fresh| self.paint(rt, fresh));
                self.target = target;
                unsafe {
                    let _ = EndPaint(hwnd, &ps);
                }
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            // The overlay spans monitors; never let a DPI change resize it.
            WM_DPICHANGED => Some(LRESULT(0)),
            WM_SETCURSOR => {
                let p = ui::cursor_pos(hwnd);
                ui::set_cursor(if self.bar_rect.contains(p) {
                    IDC_ARROW
                } else if self.mode == Mode::Window {
                    IDC_HAND
                } else {
                    IDC_CROSS
                });
                Some(LRESULT(1))
            }
            WM_MOUSEMOVE => {
                self.cur = ui::xy(lp);
                self.hover_btn = if self.drag.is_none() { self.button_at(self.cur) } else { None };
                self.update_hover();
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let p = ui::xy(lp);
                self.cur = p;
                if let Some(b) = self.button_at(p) {
                    self.pressed = Some(b);
                } else if self.mode == Mode::Region {
                    self.drag = Some(p);
                    unsafe { SetCapture(hwnd) };
                }
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let p = ui::xy(lp);
                self.cur = p;
                if let Some(b) = self.pressed.take() {
                    if self.button_at(p) == Some(b) {
                        self.press_bar(hwnd, b);
                    }
                    return Some(LRESULT(0));
                }
                match self.mode {
                    Mode::Region => {
                        if let Some(r) = self.selection() {
                            self.drag = None;
                            unsafe {
                                let _ = ReleaseCapture();
                            }
                            if r.w() >= MIN_DRAG && r.h() >= MIN_DRAG {
                                self.finish(hwnd, Some(r.intersect(&self.full())));
                            }
                        }
                    }
                    Mode::Window => {
                        self.update_hover();
                        if let Some(r) = self.selection() {
                            self.finish(hwnd, Some(r));
                        }
                    }
                    Mode::Full => {}
                }
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_RBUTTONUP => {
                if self.drag.take().is_some() {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    ui::invalidate(hwnd);
                } else {
                    self.finish(hwnd, None);
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = VIRTUAL_KEY(wp.0 as u16);
                match vk {
                    VK_ESCAPE => self.finish(hwnd, None),
                    VK_RETURN if self.mode == Mode::Window => {
                        if let Some(r) = self.selection() {
                            self.finish(hwnd, Some(r));
                        }
                    }
                    _ => match wp.0 as u8 {
                        b'R' => self.press_bar(hwnd, Bar::Mode(Mode::Region)),
                        b'W' => self.press_bar(hwnd, Bar::Mode(Mode::Window)),
                        b'F' => self.press_bar(hwnd, Bar::Mode(Mode::Full)),
                        _ => {}
                    },
                }
                Some(LRESULT(0))
            }
            WM_ACTIVATE => {
                // Keep keyboard focus for Esc even if activation arrived late.
                if (wp.0 & 0xFFFF) as u32 != WA_INACTIVE {
                    unsafe {
                        let _ = SetFocus(Some(hwnd));
                    }
                }
                None
            }
            _ => None,
        }
    }
}
