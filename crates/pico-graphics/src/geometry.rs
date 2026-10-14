/// A point in pixel coordinates (`pimoroni::Point`).
///
/// `x` grows rightwards and `y` downwards from the top-left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    /// Horizontal position.
    pub x: i32,
    /// Vertical position.
    pub y: i32,
}

impl Point {
    /// Create a point.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Clamp the point so it lies inside `r` (`Point.clamp`).
    ///
    /// ```
    /// use pico_graphics::{Point, Rect};
    /// let screen = Rect::new(0, 0, 53, 11);
    /// assert_eq!(Point::new(10, 1000).clamp(screen), Point::new(10, 10));
    /// ```
    pub fn clamp(self, r: Rect) -> Self {
        Self {
            x: self.x.clamp(r.x, (r.x + r.w - 1).max(r.x)),
            y: self.y.clamp(r.y, (r.y + r.h - 1).max(r.y)),
        }
    }
}

/// Component-wise addition.
impl core::ops::Add for Point {
    type Output = Point;
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}

/// Component-wise subtraction.
impl core::ops::Sub for Point {
    type Output = Point;
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}

/// An axis-aligned rectangle (`pimoroni::Rect`).
///
/// `x`, `y` is the top-left corner; the rectangle covers `x..x + w` and `y..y + h`
/// (the right and bottom edges are exclusive).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Rect {
    /// Create a rectangle from its top-left corner and size.
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// True if the width or height is zero or negative (`Rect.empty`).
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// True if `p` is inside; the right and bottom edges are exclusive (`Rect.contains`).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }

    /// True if the two rectangles overlap (`Rect.intersects`).
    pub fn intersects(&self, r: Rect) -> bool {
        !(self.x >= r.x + r.w || self.y >= r.y + r.h || self.x + self.w <= r.x || self.y + self.h <= r.y)
    }

    /// The overlapping area, or an empty rectangle if they do not overlap
    /// (`Rect.intersection`).
    pub fn intersection(&self, r: Rect) -> Rect {
        let x = self.x.max(r.x);
        let y = self.y.max(r.y);
        let right = (self.x + self.w).min(r.x + r.w);
        let bottom = (self.y + self.h).min(r.y + r.h);
        if right <= x || bottom <= y {
            Rect::default()
        } else {
            Rect::new(x, y, right - x, bottom - y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_half_open() {
        let r = Rect::new(0, 0, 2, 2);
        assert!(r.contains(Point::new(1, 1)));
        assert!(!r.contains(Point::new(2, 1)));
    }

    #[test]
    fn intersection_of_disjoint_rects_is_empty() {
        assert!(Rect::new(0, 0, 2, 2).intersection(Rect::new(5, 5, 2, 2)).is_empty());
    }

    #[test]
    fn intersection_overlaps() {
        assert_eq!(Rect::new(0, 0, 4, 4).intersection(Rect::new(2, 2, 4, 4)), Rect::new(2, 2, 2, 2));
    }

    #[test]
    fn clamp_pulls_point_inside() {
        assert_eq!(Point::new(-3, 99).clamp(Rect::new(0, 0, 10, 5)), Point::new(0, 4));
    }
}
