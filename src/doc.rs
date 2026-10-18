//! The markup document: the captured image, editable shapes on top of it, a
//! non-destructive crop, and snapshot-based undo/redo.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::core::*;

use crate::geom::*;
use crate::gfx::*;
use crate::settings::FontSpec;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Rect,
    Arrow,
    Text,
    Highlight,
    Pen,
    Crop,
}

impl Tool {
    /// Index into the per-tool colour/size settings, for tools that draw.
    pub fn slot(self) -> Option<usize> {
        match self {
            Tool::Rect => Some(0),
            Tool::Arrow => Some(1),
            Tool::Text => Some(2),
            Tool::Highlight => Some(3),
            Tool::Pen => Some(4),
            Tool::Select | Tool::Crop => None,
        }
    }
    pub fn has_size(self) -> bool {
        matches!(self, Tool::Rect | Tool::Arrow | Tool::Text | Tool::Pen)
    }
}

pub const STROKES: [f32; 3] = [2.0, 4.0, 8.0];
pub const FONT_SIZES: [f32; 3] = [16.0, 24.0, 40.0];
pub const HIGHLIGHT_ALPHA: f32 = 0.4;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Rect { a: Pt, b: Pt },
    Highlight { a: Pt, b: Pt },
    Arrow { a: Pt, b: Pt },
    Pen { pts: Vec<Pt> },
    Text { at: Pt, text: String, font: FontSpec },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    pub color: Color,
    /// Index into `STROKES` (or `FONT_SIZES` for text).
    pub size: usize,
}

impl Shape {
    pub fn tool(&self) -> Tool {
        match self.kind {
            Kind::Rect { .. } => Tool::Rect,
            Kind::Highlight { .. } => Tool::Highlight,
            Kind::Arrow { .. } => Tool::Arrow,
            Kind::Pen { .. } => Tool::Pen,
            Kind::Text { .. } => Tool::Text,
        }
    }

    pub fn width(&self) -> f32 {
        STROKES[self.size.min(2)]
    }

    pub fn bounds(&self) -> Rect {
        let w = self.width() / 2.0;
        match &self.kind {
            Kind::Rect { a, b } => Rect::from_pts(*a, *b).inflate(w),
            Kind::Highlight { a, b } => Rect::from_pts(*a, *b),
            Kind::Arrow { a, b } => {
                let (_, head) = arrow_parts(*a, *b, self.width());
                head.iter().fold(Rect::from_pts(*a, *b), |r, p| r.union(&Rect::from_pts(*p, *p))).inflate(w)
            }
            Kind::Pen { pts } => {
                let first = pts.first().copied().unwrap_or_default();
                pts.iter().fold(Rect::from_pts(first, first), |r, p| r.union(&Rect::from_pts(*p, *p))).inflate(w)
            }
            Kind::Text { at, text, font } => {
                let (tw, th) = measure_text(text, font, self.size);
                Rect::xywh(at.x, at.y, tw.max(4.0), th)
            }
        }
    }

    /// Is `p` on the shape? Outline rectangles only hit on their border so
    /// that shapes inside them stay clickable.
    pub fn hit(&self, p: Pt, tol: f32) -> bool {
        let w = self.width() / 2.0 + tol;
        match &self.kind {
            Kind::Rect { a, b } => {
                let r = Rect::from_pts(*a, *b);
                r.inflate(w).contains(p) && !r.inflate(-w).contains(p)
            }
            Kind::Highlight { a, b } => Rect::from_pts(*a, *b).inflate(tol).contains(p),
            Kind::Arrow { a, b } => {
                dist_to_seg(p, *a, *b) <= w.max(self.width() * 1.5 + tol)
                    || self.bounds().contains(p) && p.dist(*b) < self.width() * 3.0 + 8.0
            }
            Kind::Pen { pts } => {
                if pts.len() == 1 {
                    return p.dist(pts[0]) <= w;
                }
                pts.windows(2).any(|s| dist_to_seg(p, s[0], s[1]) <= w)
            }
            Kind::Text { .. } => self.bounds().inflate(tol).contains(p),
        }
    }

    pub fn translate(&mut self, d: Pt) {
        match &mut self.kind {
            Kind::Rect { a, b } | Kind::Highlight { a, b } | Kind::Arrow { a, b } => {
                *a = *a + d;
                *b = *b + d;
            }
            Kind::Pen { pts } => pts.iter_mut().for_each(|p| *p = *p + d),
            Kind::Text { at, .. } => *at = *at + d,
        }
    }

    /// Resize handles: eight for boxes (clockwise from top-left), two for arrows.
    pub fn handles(&self) -> Vec<Pt> {
        match &self.kind {
            Kind::Rect { a, b } | Kind::Highlight { a, b } => {
                let r = Rect::from_pts(*a, *b);
                let c = r.center();
                vec![
                    pt(r.l, r.t),
                    pt(c.x, r.t),
                    pt(r.r, r.t),
                    pt(r.r, c.y),
                    pt(r.r, r.b),
                    pt(c.x, r.b),
                    pt(r.l, r.b),
                    pt(r.l, c.y),
                ]
            }
            Kind::Arrow { a, b } => vec![*a, *b],
            _ => Vec::new(),
        }
    }

    /// Apply a handle drag to `orig`, writing the result into `self`.
    pub fn drag_handle(&mut self, orig: &Shape, h: usize, p: Pt) {
        match (&mut self.kind, &orig.kind) {
            (Kind::Rect { a, b }, Kind::Rect { a: oa, b: ob })
            | (Kind::Highlight { a, b }, Kind::Highlight { a: oa, b: ob }) => {
                let mut r = Rect::from_pts(*oa, *ob);
                match h {
                    0 => (r.l, r.t) = (p.x, p.y),
                    1 => r.t = p.y,
                    2 => (r.r, r.t) = (p.x, p.y),
                    3 => r.r = p.x,
                    4 => (r.r, r.b) = (p.x, p.y),
                    5 => r.b = p.y,
                    6 => (r.l, r.b) = (p.x, p.y),
                    _ => r.l = p.x,
                }
                *a = pt(r.l, r.t);
                *b = pt(r.r, r.b);
            }
            (Kind::Arrow { a, b }, Kind::Arrow { .. }) => {
                if h == 0 {
                    *a = p
                } else {
                    *b = p
                }
            }
            _ => {}
        }
    }

    /// Too small to keep after the initial drag?
    pub fn is_degenerate(&self) -> bool {
        match &self.kind {
            Kind::Rect { a, b } | Kind::Highlight { a, b } => {
                let r = Rect::from_pts(*a, *b);
                r.w() < 3.0 || r.h() < 3.0
            }
            Kind::Arrow { a, b } => a.dist(*b) < 4.0,
            Kind::Pen { pts } => pts.is_empty(),
            Kind::Text { text, .. } => text.is_empty(),
        }
    }
}

/// Arrow geometry: where the shaft ends, and the head triangle (tip first).
pub fn arrow_parts(a: Pt, b: Pt, w: f32) -> (Pt, [Pt; 3]) {
    let len = a.dist(b);
    if len < 0.01 {
        return (b, [b; 3]);
    }
    let dir = (b - a) * (1.0 / len);
    let perp = pt(-dir.y, dir.x);
    let head = (w * 3.0 + 8.0).min(len);
    let base = b - dir * head;
    let half = head * 0.5;
    (base + dir * (head * 0.3), [b, base + perp * half, base - perp * half])
}

thread_local! {
    static FORMATS: RefCell<HashMap<(FontSpec, usize), IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

pub fn annotation_format(font: &FontSpec, size: usize) -> Option<IDWriteTextFormat> {
    let size = size.min(2);
    let key = (font.clone(), size);
    if let Some(f) = FORMATS.with(|f| f.borrow().get(&key).cloned()) {
        return Some(f);
    }
    let weight = if font.bold { DWRITE_FONT_WEIGHT_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
    let style = if font.italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL };
    let fmt = unsafe {
        gfx().dw.CreateTextFormat(
            &HSTRING::from(font.family.as_str()),
            None,
            weight,
            style,
            DWRITE_FONT_STRETCH_NORMAL,
            FONT_SIZES[size],
            w!("en-us"),
        )
    }
    .ok()?;
    FORMATS.with(|f| f.borrow_mut().insert(key, fmt.clone()));
    Some(fmt)
}

pub fn measure_text(text: &str, font: &FontSpec, size: usize) -> (f32, f32) {
    annotation_format(font, size)
        .and_then(|f| text_layout(text, &f).ok())
        .map(|l| layout_size(&l))
        .unwrap_or((0.0, FONT_SIZES[size.min(2)] * 1.33))
}

pub fn draw_shape(c: &Canvas, s: &Shape) {
    let w = s.width();
    match &s.kind {
        Kind::Rect { a, b } => c.stroke(Rect::from_pts(*a, *b), s.color, 1.0, w),
        Kind::Highlight { a, b } => c.fill(Rect::from_pts(*a, *b), s.color, HIGHLIGHT_ALPHA),
        Kind::Arrow { a, b } => {
            let (end, head) = arrow_parts(*a, *b, w);
            c.line(*a, end, s.color, 1.0, w);
            if let Ok(g) = poly(&head, true, true) {
                c.fill_geom(&g, s.color, 1.0);
                c.stroke_geom(&g, s.color, 1.0, 1.0);
            }
        }
        Kind::Pen { pts } => match pts.len() {
            0 => {}
            1 => c.fill_circle(pts[0], w / 2.0, s.color, 1.0),
            _ => {
                if let Ok(g) = poly(pts, false, false) {
                    c.stroke_geom(&g, s.color, 1.0, w);
                }
            }
        },
        Kind::Text { at, text, font } => {
            if let Some(l) = annotation_format(font, s.size).and_then(|f| text_layout(text, &f).ok()) {
                c.layout(&l, *at, s.color, 1.0);
            }
        }
    }
}

pub fn draw_shapes(c: &Canvas, shapes: &[Shape]) {
    unsafe { c.rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE) };
    for s in shapes {
        draw_shape(c, s);
    }
    unsafe { c.rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_DEFAULT) };
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub shapes: Vec<Shape>,
    /// Visible part of the image, in image pixels.
    pub crop: Rect,
}

pub struct Doc {
    pub image: Image,
    pub state: State,
    undo: Vec<State>,
    redo: Vec<State>,
}

const HISTORY_LIMIT: usize = 200;

impl Doc {
    pub fn new(image: Image) -> Doc {
        let crop = Rect::new(0.0, 0.0, image.w as f32, image.h as f32);
        Doc { image, state: State { shapes: Vec::new(), crop }, undo: Vec::new(), redo: Vec::new() }
    }

    /// Record the current state before a change.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.state.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Drop the last checkpoint if nothing changed since (e.g. a click without a drag).
    pub fn settle(&mut self) {
        if self.undo.last() == Some(&self.state) {
            self.undo.pop();
        }
    }

    /// Abandon the change in progress: back to the last checkpoint, no redo entry.
    pub fn revert(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.state = prev;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.state, prev));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.state, next));
        true
    }

    /// Topmost shape under `p`.
    pub fn hit(&self, p: Pt, tol: f32) -> Option<usize> {
        self.state.shapes.iter().rposition(|s| s.hit(p, tol))
    }

    pub fn is_dirty(&self) -> bool {
        !self.state.shapes.is_empty() || self.state.crop != Rect::new(0.0, 0.0, self.image.w as f32, self.image.h as f32)
    }

    /// The cropped image with every shape burned in.
    pub fn flatten(&self) -> Result<Image> {
        let crop = self.state.crop.round();
        if !self.is_dirty() {
            return Ok(self.image.clone());
        }
        let (w, h) = (crop.w().max(1.0) as u32, crop.h().max(1.0) as u32);
        render_offscreen(w, h, |rt| {
            let c = Canvas::new(rt)?;
            c.set_transform(1.0, -crop.l, -crop.t);
            let bmp = self.image.to_bitmap(rt)?;
            let full = Rect::new(0.0, 0.0, self.image.w as f32, self.image.h as f32);
            unsafe {
                rt.DrawBitmap(&bmp, Some(&full.d2d()), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None);
            }
            draw_shapes(&c, &self.state.shapes);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(a: Pt, b: Pt) -> Shape {
        Shape { kind: Kind::Rect { a, b }, color: Color(0xFF0000), size: 0 }
    }

    #[test]
    fn outline_rect_hits_only_its_border() {
        let s = rect(pt(10.0, 10.0), pt(110.0, 60.0));
        assert!(s.hit(pt(10.0, 30.0), 3.0));
        assert!(s.hit(pt(60.0, 61.0), 3.0));
        assert!(!s.hit(pt(60.0, 35.0), 3.0));
        assert!(!s.hit(pt(200.0, 35.0), 3.0));
    }

    #[test]
    fn handle_drag_moves_one_corner() {
        let orig = rect(pt(10.0, 10.0), pt(110.0, 60.0));
        let mut s = orig.clone();
        s.drag_handle(&orig, 4, pt(150.0, 90.0));
        assert_eq!(s.kind, Kind::Rect { a: pt(10.0, 10.0), b: pt(150.0, 90.0) });
        s.drag_handle(&orig, 1, pt(999.0, 0.0));
        assert_eq!(s.kind, Kind::Rect { a: pt(10.0, 0.0), b: pt(110.0, 60.0) });
    }

    #[test]
    fn arrow_head_points_at_target() {
        let (end, head) = arrow_parts(pt(0.0, 0.0), pt(100.0, 0.0), 4.0);
        assert_eq!(head[0], pt(100.0, 0.0));
        assert!(end.x < 100.0 && end.x > 70.0);
        assert_eq!(head[1].x, head[2].x);
        assert_eq!(head[1].y, -head[2].y);
    }

    #[test]
    fn undo_redo_and_settle() {
        let mut d = Doc::new(Image { w: 4, h: 4, px: vec![0; 64] });
        d.checkpoint();
        d.state.shapes.push(rect(pt(0.0, 0.0), pt(2.0, 2.0)));
        assert!(d.can_undo());
        assert!(d.undo());
        assert!(d.state.shapes.is_empty());
        assert!(d.redo());
        assert_eq!(d.state.shapes.len(), 1);

        d.checkpoint();
        d.settle();
        assert!(d.undo());
        assert!(!d.can_undo());
    }

    #[test]
    fn topmost_shape_wins() {
        let mut d = Doc::new(Image { w: 4, h: 4, px: vec![0; 64] });
        d.state.shapes.push(rect(pt(0.0, 0.0), pt(100.0, 100.0)));
        d.state.shapes.push(Shape {
            kind: Kind::Highlight { a: pt(-10.0, -10.0), b: pt(20.0, 20.0) },
            color: Color(0xFFFF00),
            size: 0,
        });
        assert_eq!(d.hit(pt(0.0, 5.0), 2.0), Some(1));
        assert_eq!(d.hit(pt(100.0, 50.0), 2.0), Some(0));
        assert_eq!(d.hit(pt(50.0, 50.0), 2.0), None);
    }
}
