//! Direct2D / DirectWrite / WIC plumbing: shared factories, a thin drawing
//! wrapper, per-window render targets, off-screen rendering and PNG encoding.

use std::ptr::null_mut;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::StructuredStorage::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::*;
use windows_numerics::{Matrix3x2, Vector2};

use crate::geom::*;

pub struct Gfx {
    pub d2d: ID2D1Factory,
    pub dw: IDWriteFactory,
    pub wic: IWICImagingFactory,
    /// Round caps and joins: arrows, pen strokes, icons.
    pub round: ID2D1StrokeStyle,
    /// Dashed hairline for selection frames.
    pub dash: ID2D1StrokeStyle,
}

thread_local! {
    static GFX: &'static Gfx = Box::leak(Box::new(Gfx::new().expect("graphics initialisation failed")));
}

/// The UI thread's graphics factories. Everything in Snipper runs on one thread.
pub fn gfx() -> &'static Gfx {
    GFX.with(|g| *g)
}

impl Gfx {
    fn new() -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let round = d2d.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                },
                None,
            )?;
            let dash = d2d.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_FLAT,
                    endCap: D2D1_CAP_STYLE_FLAT,
                    dashCap: D2D1_CAP_STYLE_FLAT,
                    lineJoin: D2D1_LINE_JOIN_MITER,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_CUSTOM,
                    dashOffset: 0.0,
                },
                Some(&[4.0, 3.0]),
            )?;
            Ok(Gfx { d2d, dw, wic, round, dash })
        }
    }
}

/// An opaque RGB colour, `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    pub const WHITE: Color = Color(0xFFFFFF);
    pub const BLACK: Color = Color(0x000000);

    pub fn f(self, a: f32) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: ((self.0 >> 16) & 0xFF) as f32 / 255.0,
            g: ((self.0 >> 8) & 0xFF) as f32 / 255.0,
            b: (self.0 & 0xFF) as f32 / 255.0,
            a,
        }
    }
    /// Perceived brightness, 0–255; used to pick a contrasting outline.
    pub fn luma(self) -> u32 {
        let (r, g, b) = ((self.0 >> 16) & 0xFF, (self.0 >> 8) & 0xFF, self.0 & 0xFF);
        (r * 299 + g * 587 + b * 114) / 1000
    }
    /// GDI `COLORREF` (0x00BBGGRR).
    pub fn colorref(self) -> u32 {
        let (r, g, b) = ((self.0 >> 16) & 0xFF, (self.0 >> 8) & 0xFF, self.0 & 0xFF);
        r | (g << 8) | (b << 16)
    }
    pub fn from_colorref(c: u32) -> Color {
        let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
        Color((r << 16) | (g << 8) | b)
    }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub fn text_format(size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
    unsafe {
        gfx().dw.CreateTextFormat(
            w!("Segoe UI"),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )
    }
}

/// A text format centred both ways in its layout box; used for labels.
pub fn centered_format(size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
    let f = text_format(size, weight)?;
    unsafe {
        f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    }
    Ok(f)
}

/// Text layout without wrapping, for annotations and measured labels.
pub fn text_layout(s: &str, fmt: &IDWriteTextFormat) -> Result<IDWriteTextLayout> {
    unsafe { gfx().dw.CreateTextLayout(&wide(s), fmt, 100_000.0, 100_000.0) }
}

pub fn layout_size(l: &IDWriteTextLayout) -> (f32, f32) {
    let mut m = DWRITE_TEXT_METRICS::default();
    unsafe {
        let _ = l.GetMetrics(&mut m);
    }
    (m.widthIncludingTrailingWhitespace, m.height)
}

/// Drawing helper over any render target, reusing one solid brush.
pub struct Canvas<'a> {
    pub rt: &'a ID2D1RenderTarget,
    brush: ID2D1SolidColorBrush,
}

impl<'a> Canvas<'a> {
    pub fn new(rt: &'a ID2D1RenderTarget) -> Result<Self> {
        let brush = unsafe { rt.CreateSolidColorBrush(&Color::BLACK.f(1.0), None)? };
        Ok(Canvas { rt, brush })
    }
    pub fn brush(&self, c: Color, a: f32) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&c.f(a)) };
        &self.brush
    }
    pub fn clear(&self, c: Color) {
        unsafe { self.rt.Clear(Some(&c.f(1.0))) }
    }
    pub fn fill(&self, r: Rect, c: Color, a: f32) {
        unsafe { self.rt.FillRectangle(&r.d2d(), self.brush(c, a)) }
    }
    pub fn stroke(&self, r: Rect, c: Color, a: f32, w: f32) {
        unsafe { self.rt.DrawRectangle(&r.d2d(), self.brush(c, a), w, None) }
    }
    pub fn stroke_styled(&self, r: Rect, c: Color, a: f32, w: f32, style: &ID2D1StrokeStyle) {
        unsafe { self.rt.DrawRectangle(&r.d2d(), self.brush(c, a), w, style) }
    }
    pub fn fill_round(&self, r: Rect, rad: f32, c: Color, a: f32) {
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.FillRoundedRectangle(&rr, self.brush(c, a)) }
    }
    pub fn stroke_round(&self, r: Rect, rad: f32, c: Color, a: f32, w: f32) {
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.DrawRoundedRectangle(&rr, self.brush(c, a), w, None) }
    }
    pub fn line(&self, p: Pt, q: Pt, c: Color, a: f32, w: f32) {
        unsafe { self.rt.DrawLine(p.v2(), q.v2(), self.brush(c, a), w, &gfx().round) }
    }
    pub fn fill_circle(&self, ctr: Pt, rad: f32, c: Color, a: f32) {
        let e = D2D1_ELLIPSE { point: ctr.v2(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.FillEllipse(&e, self.brush(c, a)) }
    }
    pub fn stroke_circle(&self, ctr: Pt, rad: f32, c: Color, a: f32, w: f32) {
        let e = D2D1_ELLIPSE { point: ctr.v2(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.DrawEllipse(&e, self.brush(c, a), w, None) }
    }
    pub fn fill_geom(&self, g: &ID2D1PathGeometry, c: Color, a: f32) {
        unsafe { self.rt.FillGeometry(g, self.brush(c, a), None) }
    }
    pub fn stroke_geom(&self, g: &ID2D1PathGeometry, c: Color, a: f32, w: f32) {
        unsafe { self.rt.DrawGeometry(g, self.brush(c, a), w, &gfx().round) }
    }
    pub fn text(&self, s: &str, fmt: &IDWriteTextFormat, r: Rect, c: Color, a: f32) {
        unsafe {
            self.rt.DrawText(
                &wide(s),
                fmt,
                &r.d2d(),
                self.brush(c, a),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        }
    }
    pub fn layout(&self, l: &IDWriteTextLayout, at: Pt, c: Color, a: f32) {
        unsafe { self.rt.DrawTextLayout(at.v2(), l, self.brush(c, a), D2D1_DRAW_TEXT_OPTIONS_NONE) }
    }
    pub fn set_transform(&self, scale: f32, dx: f32, dy: f32) {
        let m = Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: dx, M32: dy };
        unsafe { self.rt.SetTransform(&m) }
    }
    pub fn reset_transform(&self) {
        self.set_transform(1.0, 0.0, 0.0)
    }
    pub fn push_clip(&self, r: Rect) {
        unsafe { self.rt.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_ALIASED) }
    }
    pub fn pop_clip(&self) {
        unsafe { self.rt.PopAxisAlignedClip() }
    }
}

/// Path segments for building small geometries (icons, arrow heads).
pub enum Seg {
    M(f32, f32),
    L(f32, f32),
    C(f32, f32, f32, f32, f32, f32),
    /// Arc to (x, y) with radius r; `true` sweeps clockwise.
    A(f32, f32, f32, bool),
    Z,
}

/// Build a path from segments, mapping each point through `o + p * s`.
pub fn path(segs: &[Seg], o: Pt, s: f32, filled: bool) -> Result<ID2D1PathGeometry> {
    let v = |x: f32, y: f32| Vector2 { X: o.x + x * s, Y: o.y + y * s };
    unsafe {
        let g = gfx().d2d.CreatePathGeometry()?;
        let sink = g.Open()?;
        let begin = if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
        let mut open = false;
        for seg in segs {
            match *seg {
                Seg::M(x, y) => {
                    if open {
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    }
                    sink.BeginFigure(v(x, y), begin);
                    open = true;
                }
                Seg::L(x, y) => sink.AddLine(v(x, y)),
                Seg::C(x1, y1, x2, y2, x3, y3) => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: v(x1, y1),
                    point2: v(x2, y2),
                    point3: v(x3, y3),
                }),
                Seg::A(x, y, r, cw) => sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: v(x, y),
                    size: D2D_SIZE_F { width: r * s, height: r * s },
                    rotationAngle: 0.0,
                    sweepDirection: if cw { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                    arcSize: D2D1_ARC_SIZE_SMALL,
                }),
                Seg::Z => {
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    open = false;
                }
            }
        }
        if open {
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close()?;
        Ok(g)
    }
}

/// A polyline/polygon through absolute points.
pub fn poly(pts: &[Pt], closed: bool, filled: bool) -> Result<ID2D1PathGeometry> {
    let mut segs = Vec::with_capacity(pts.len() + 1);
    for (i, p) in pts.iter().enumerate() {
        segs.push(if i == 0 { Seg::M(p.x, p.y) } else { Seg::L(p.x, p.y) });
    }
    if closed {
        segs.push(Seg::Z);
    }
    path(&segs, Pt::default(), 1.0, filled)
}

/// 32-bit BGRA pixels, top-down, tightly packed. Alpha is always 255.
#[derive(Clone)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Image {
    /// Copy of the sub-rectangle `r` (clamped to the image).
    pub fn sub(&self, x: i32, y: i32, w: i32, h: i32) -> Image {
        let x0 = x.clamp(0, self.w as i32) as usize;
        let y0 = y.clamp(0, self.h as i32) as usize;
        let x1 = (x + w).clamp(0, self.w as i32) as usize;
        let y1 = (y + h).clamp(0, self.h as i32) as usize;
        let (nw, nh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let mut px = Vec::with_capacity(nw * nh * 4);
        let stride = self.w as usize * 4;
        for row in y0..y1 {
            px.extend_from_slice(&self.px[row * stride + x0 * 4..row * stride + x1 * 4]);
        }
        Image { w: nw as u32, h: nh as u32, px }
    }

    pub fn to_bitmap(&self, rt: &ID2D1RenderTarget) -> Result<ID2D1Bitmap> {
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        unsafe {
            rt.CreateBitmap(
                D2D_SIZE_U { width: self.w.max(1), height: self.h.max(1) },
                Some(self.px.as_ptr() as *const _),
                self.w * 4,
                &props,
            )
        }
    }
}

/// A Direct2D render target bound to a window, in physical pixels (96 DPI,
/// so one DIP is one pixel and callers scale UI metrics themselves).
#[derive(Default)]
pub struct WindowTarget {
    rt: Option<ID2D1HwndRenderTarget>,
}

impl WindowTarget {
    /// Returns the target, creating it if needed. `fresh` tells the caller that
    /// device resources (bitmaps) must be recreated.
    pub fn get(&mut self, hwnd: HWND) -> Result<(ID2D1RenderTarget, bool)> {
        if let Some(rt) = &self.rt {
            return Ok((rt.cast()?, false));
        }
        let mut rc = RECT::default();
        unsafe { GetClientRect(hwnd, &mut rc)? };
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U { width: (rc.right - rc.left).max(1) as u32, height: (rc.bottom - rc.top).max(1) as u32 },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let rt = unsafe { gfx().d2d.CreateHwndRenderTarget(&props, &hprops)? };
        let base: ID2D1RenderTarget = rt.cast()?;
        self.rt = Some(rt);
        Ok((base, true))
    }

    pub fn resize(&self, w: u32, h: u32) {
        if let Some(rt) = &self.rt {
            unsafe {
                let _ = rt.Resize(&D2D_SIZE_U { width: w.max(1), height: h.max(1) });
            }
        }
    }

    /// Run `draw` inside BeginDraw/EndDraw; drops the target if the device was lost.
    pub fn paint(&mut self, hwnd: HWND, draw: impl FnOnce(&ID2D1RenderTarget, bool) -> Result<()>) {
        let Ok((rt, fresh)) = self.get(hwnd) else { return };
        unsafe {
            rt.BeginDraw();
            let r = draw(&rt, fresh);
            let end = rt.EndDraw(None, None);
            if r.is_err() || end.as_ref().is_err_and(|e| e.code() == D2DERR_RECREATE_TARGET) {
                self.rt = None;
            }
        }
    }
}

/// Render into an off-screen bitmap and read the pixels back.
pub fn render_offscreen(w: u32, h: u32, draw: impl FnOnce(&ID2D1RenderTarget) -> Result<()>) -> Result<Image> {
    unsafe {
        let g = gfx();
        let bmp = g.wic.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rt = g.d2d.CreateWicBitmapRenderTarget(&bmp, &props)?;
        rt.BeginDraw();
        let drawn = draw(&rt);
        rt.EndDraw(None, None)?;
        drawn?;
        drop(rt);

        let lock = bmp.Lock(&WICRect { X: 0, Y: 0, Width: w as i32, Height: h as i32 }, WICBitmapLockRead.0 as u32)?;
        let stride = lock.GetStride()? as usize;
        let mut size = 0u32;
        let mut data = null_mut();
        lock.GetDataPointer(&mut size, &mut data)?;
        let src = std::slice::from_raw_parts(data, size as usize);
        let row = w as usize * 4;
        let mut px = Vec::with_capacity(row * h as usize);
        for y in 0..h as usize {
            px.extend_from_slice(&src[y * stride..y * stride + row]);
        }
        for p in px.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
        Ok(Image { w, h, px })
    }
}

/// Encode as a 24-bit PNG.
pub fn encode_png(img: &Image) -> Result<Vec<u8>> {
    unsafe {
        let g = gfx();
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let enc = g.wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut bag = None;
        enc.CreateNewFrame(&mut frame, &mut bag)?;
        let frame = frame.ok_or_else(|| Error::from(E_FAIL))?;
        frame.Initialize(bag.as_ref())?;
        frame.SetSize(img.w, img.h)?;
        let mut fmt = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut fmt)?;
        if fmt == GUID_WICPixelFormat24bppBGR {
            let stride = (img.w as usize * 3 + 3) & !3;
            let mut buf = vec![0u8; stride * img.h as usize];
            for y in 0..img.h as usize {
                let src = &img.px[y * img.w as usize * 4..(y + 1) * img.w as usize * 4];
                let dst = &mut buf[y * stride..y * stride + img.w as usize * 3];
                for (d, s) in dst.as_chunks_mut::<3>().0.iter_mut().zip(src.chunks_exact(4)) {
                    d.copy_from_slice(&s[..3]);
                }
            }
            frame.WritePixels(img.h, stride as u32, &buf)?;
        } else {
            let mut fmt = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&mut fmt)?;
            frame.WritePixels(img.h, img.w * 4, &img.px)?;
        }
        frame.Commit()?;
        enc.Commit()?;

        let mut len = 0u64;
        stream.Seek(0, STREAM_SEEK_END, Some(&mut len))?;
        let hg = GetHGlobalFromStream(&stream)?;
        let p = GlobalLock(hg) as *const u8;
        if p.is_null() {
            return Err(Error::from_thread());
        }
        let bytes = std::slice::from_raw_parts(p, len as usize).to_vec();
        let _ = GlobalUnlock(hg);
        Ok(bytes)
    }
}
