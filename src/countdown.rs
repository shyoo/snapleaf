//! The delay countdown: a small always-on-top tile showing the seconds left.

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::app;
use crate::geom::*;
use crate::gfx::*;
use crate::ui::{self, Handler, WindowSpec};

const WM_SET_COUNT: u32 = WM_USER + 1;

struct Countdown {
    n: u32,
    scale: f32,
    target: WindowTarget,
}

pub fn show(seconds: u32) -> Result<HWND> {
    let (work, scale) = ui::cursor_monitor();
    let size = (150.0 * scale) as i32;
    let c = work.center();
    let hwnd = ui::create_window(
        WindowSpec {
            class: w!("SnapleafCountdown"),
            title: "Snapleaf countdown",
            style: WS_POPUP,
            ex: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            rect: (c.x as i32 - size / 2, c.y as i32 - size / 2, size, size),
            cursor: IDC_HAND,
            owner: None,
        },
        Countdown { n: seconds, scale, target: WindowTarget::default() },
    )?;
    ui::style_frame(hwnd, true);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    Ok(hwnd)
}

pub fn set(hwnd: HWND, n: u32) {
    unsafe {
        SendMessageW(hwnd, WM_SET_COUNT, Some(WPARAM(n as usize)), None);
    }
}

impl Handler for Countdown {
    fn on(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, _lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_SET_COUNT => {
                self.n = wp.0 as u32;
                ui::invalidate(hwnd);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                unsafe {
                    let _ = PostMessageW(Some(app::host()), app::WM_APP_CANCEL, WPARAM(0), LPARAM(0));
                }
                Some(LRESULT(0))
            }
            WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                let (n, s) = (self.n, self.scale);
                self.target.paint(hwnd, |rt, _| {
                    let c = Canvas::new(rt)?;
                    let r = ui::client_rect(hwnd);
                    c.clear(Color(0x1F1F1F));
                    let big = centered_format(64.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
                    let small = centered_format(12.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;
                    c.text(&n.to_string(), &big, Rect::new(r.l, r.t, r.r, r.b - 18.0 * s), Color::WHITE, 1.0);
                    c.text("Click to cancel", &small, Rect::new(r.l, r.b - 40.0 * s, r.r, r.b - 12.0 * s), Color::WHITE, 0.6);
                    Ok(())
                });
                unsafe {
                    let _ = EndPaint(hwnd, &ps);
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}
