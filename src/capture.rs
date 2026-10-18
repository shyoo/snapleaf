//! Freezing the desktop: one GDI grab of the whole virtual screen, plus the
//! visible top-level window rectangles at that instant for window mode.

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::geom::Rect;
use crate::gfx::Image;

pub struct Frozen {
    pub img: Image,
    /// Virtual-screen origin: screen coordinate of pixel (0, 0).
    pub x: i32,
    pub y: i32,
    /// Visible windows, front to back, in image coordinates.
    pub windows: Vec<Rect>,
}

pub fn freeze() -> Result<Frozen> {
    let windows = visible_windows();
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let img = grab(x, y, w, h)?;
        let windows = windows
            .into_iter()
            .map(|r| r.offset(crate::geom::pt(-x as f32, -y as f32)))
            .map(|r| r.intersect(&Rect::new(0.0, 0.0, w as f32, h as f32)))
            .filter(|r| !r.is_empty())
            .collect();
        Ok(Frozen { img, x, y, windows })
    }
}

unsafe fn grab(x: i32, y: i32, w: i32, h: i32) -> Result<Image> {
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let result = (|| {
            let dib = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let old = SelectObject(mem, dib.into());
            let blt = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
            let mut px = Vec::new();
            if blt.is_ok() {
                px = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
                for p in px.as_chunks_mut::<4>().0 {
                    p[3] = 255;
                }
            }
            SelectObject(mem, old);
            let _ = DeleteObject(dib.into());
            blt.map(|_| Image { w: w as u32, h: h as u32, px })
        })();
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        result
    }
}

fn visible_windows() -> Vec<Rect> {
    unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lp.0 as *mut Vec<HWND>) };
        list.push(hwnd);
        BOOL(1)
    }
    let mut hwnds: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut _ as isize));
    }
    hwnds.into_iter().filter_map(window_rect).collect()
}

fn window_rect(hwnd: HWND) -> Option<Rect> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut c_void, 4);
        if cloaked != 0 {
            return None;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TRANSPARENT.0 != 0 {
            return None;
        }
        if ex & WS_EX_TOOLWINDOW.0 != 0 && GetWindowTextLengthW(hwnd) == 0 {
            return None;
        }
        if ex & WS_EX_LAYERED.0 != 0 {
            let mut alpha = 255u8;
            let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
            if GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), Some(&mut flags)).is_ok()
                && flags.0 & LWA_ALPHA.0 != 0
                && alpha == 0
            {
                return None;
            }
        }
        let mut r = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as *mut c_void,
            size_of::<RECT>() as u32,
        )
        .is_err()
        {
            GetWindowRect(hwnd, &mut r).ok()?;
        }
        if r.right - r.left < 8 || r.bottom - r.top < 8 {
            return None;
        }
        Some(Rect::new(r.left as f32, r.top as f32, r.right as f32, r.bottom as f32))
    }
}
