//! Small 2D geometry helpers shared by the overlay, the editor and the renderer.

use std::ops::{Add, Mul, Sub};

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows_numerics::Vector2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

pub const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

impl Pt {
    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    pub fn dist(self, o: Pt) -> f32 {
        (self - o).len()
    }
    pub fn v2(self) -> Vector2 {
        Vector2 { X: self.x, Y: self.y }
    }
}

impl Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        pt(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        pt(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Pt {
    type Output = Pt;
    fn mul(self, k: f32) -> Pt {
        pt(self.x * k, self.y * k)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Rect {
    pub const fn new(l: f32, t: f32, r: f32, b: f32) -> Rect {
        Rect { l, t, r, b }
    }
    pub fn xywh(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, x + w, y + h)
    }
    /// Normalised rectangle spanning two arbitrary corners.
    pub fn from_pts(a: Pt, b: Pt) -> Rect {
        Rect::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
    }
    pub fn w(&self) -> f32 {
        self.r - self.l
    }
    pub fn h(&self) -> f32 {
        self.b - self.t
    }
    pub fn center(&self) -> Pt {
        pt((self.l + self.r) / 2.0, (self.t + self.b) / 2.0)
    }
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.l && p.x < self.r && p.y >= self.t && p.y < self.b
    }
    pub fn inflate(&self, d: f32) -> Rect {
        Rect::new(self.l - d, self.t - d, self.r + d, self.b + d)
    }
    pub fn offset(&self, d: Pt) -> Rect {
        Rect::new(self.l + d.x, self.t + d.y, self.r + d.x, self.b + d.y)
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        Rect::new(self.l.max(o.l), self.t.max(o.t), self.r.min(o.r), self.b.min(o.b))
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(self.l.min(o.l), self.t.min(o.t), self.r.max(o.r), self.b.max(o.b))
    }
    pub fn is_empty(&self) -> bool {
        self.w() <= 0.0 || self.h() <= 0.0
    }
    pub fn round(&self) -> Rect {
        Rect::new(self.l.round(), self.t.round(), self.r.round(), self.b.round())
    }
    pub fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.l, top: self.t, right: self.r, bottom: self.b }
    }
}

/// Shortest distance from `p` to the segment `a`–`b`.
pub fn dist_to_seg(p: Pt, a: Pt, b: Pt) -> f32 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= f32::EPSILON {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance() {
        assert_eq!(dist_to_seg(pt(5.0, 3.0), pt(0.0, 0.0), pt(10.0, 0.0)), 3.0);
        assert_eq!(dist_to_seg(pt(-4.0, 3.0), pt(0.0, 0.0), pt(10.0, 0.0)), 5.0);
        assert_eq!(dist_to_seg(pt(1.0, 1.0), pt(0.0, 0.0), pt(0.0, 0.0)), 2f32.sqrt());
    }

    #[test]
    fn rect_normalises_and_intersects() {
        let r = Rect::from_pts(pt(10.0, 20.0), pt(0.0, 5.0));
        assert_eq!(r, Rect::new(0.0, 5.0, 10.0, 20.0));
        let i = r.intersect(&Rect::new(5.0, 0.0, 50.0, 10.0));
        assert_eq!(i, Rect::new(5.0, 5.0, 10.0, 10.0));
        assert!(Rect::new(0.0, 0.0, 10.0, 10.0).intersect(&Rect::new(20.0, 0.0, 30.0, 5.0)).is_empty());
    }
}
