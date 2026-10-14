use embedded_graphics_core::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::{Rgb888, RgbColor},
};

use crate::{Point, Rect, Rgb};

/// RGB888 framebuffer with a current pen (`pimoroni::PicoGraphics_PenRGB888`).
///
/// Pixels are stored as `0x00RRGGBB`, row-major, `W` wide and `H` high.
pub struct PenRgb888<const W: usize, const H: usize> {
    frame_buffer: [[u32; W]; H],
    color: u32,
    /// The whole surface: `0, 0, W, H`.
    pub bounds: Rect,
    /// Drawing is limited to this rectangle; defaults to `bounds`.
    pub clip: Rect,
}

impl<const W: usize, const H: usize> PenRgb888<W, H> {
    /// A black framebuffer with the pen set to black (`0`) and no clip.
    pub const fn new() -> Self {
        let bounds = Rect::new(0, 0, W as i32, H as i32);
        Self {
            frame_buffer: [[0; W]; H],
            color: 0,
            bounds,
            clip: bounds,
        }
    }

    /// Row-major pixels, `0x00RRGGBB`.
    pub fn frame_buffer(&self) -> &[[u32; W]; H] {
        &self.frame_buffer
    }

    /// Set the pen to a packed `0x00RRGGBB` colour (`set_pen(uint c)`).
    pub fn set_pen(&mut self, c: u32) {
        self.color = c;
    }

    /// Set the pen from channels (`set_pen(r, g, b)`).
    pub fn set_pen_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.color = Rgb::new(r, g, b).to_rgb888();
    }

    /// Pack a colour for [`set_pen`](Self::set_pen). With RGB888 there is no palette,
    /// so this is just the packed value (`create_pen`).
    pub fn create_pen(&self, r: u8, g: u8, b: u8) -> u32 {
        Rgb::new(r, g, b).to_rgb888()
    }

    /// Like [`create_pen`](Self::create_pen) from hue, saturation and value in
    /// `0.0..=1.0` (`create_pen_hsv`).
    pub fn create_pen_hsv(&self, h: f32, s: f32, v: f32) -> u32 {
        Rgb::from_hsv(h, s, v).to_rgb888()
    }

    /// Limit drawing to `r`, itself limited to `bounds` (`set_clip`).
    pub fn set_clip(&mut self, r: Rect) {
        self.clip = self.bounds.intersection(r);
    }

    /// Reset the clip to `bounds` (`remove_clip`).
    pub fn remove_clip(&mut self) {
        self.clip = self.bounds;
    }

    /// Fill the whole clip rectangle with the pen.
    pub fn clear(&mut self) {
        self.rectangle(self.clip);
    }

    /// Plot one pixel with the pen; ignored outside the clip.
    pub fn set_pixel(&mut self, p: Point) {
        if self.clip.contains(p) {
            self.frame_buffer[p.y as usize][p.x as usize] = self.color;
        }
    }

    /// Plot `len` pixels rightwards from `p`; clipped.
    pub fn set_pixel_span(&mut self, p: Point, len: u32) {
        let span = Rect::new(p.x, p.y, len as i32, 1).intersection(self.clip);
        if span.is_empty() {
            return;
        }
        let row = &mut self.frame_buffer[span.y as usize];
        row[span.x as usize..(span.x + span.w) as usize].fill(self.color);
    }

    /// Fill `r` with the pen; clipped.
    pub fn rectangle(&mut self, r: Rect) {
        let r = r.intersection(self.clip);
        for y in r.y..r.y + r.h {
            self.set_pixel_span(Point::new(r.x, y), r.w as u32);
        }
    }
}

impl<const W: usize, const H: usize> Default for PenRgb888<W, H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const W: usize, const H: usize> OriginDimensions for PenRgb888<W, H> {
    fn size(&self) -> Size {
        Size::new(W as u32, H as u32)
    }
}

impl<const W: usize, const H: usize> DrawTarget for PenRgb888<W, H> {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let pen = self.color;
        for Pixel(p, c) in pixels {
            self.color = Rgb::new(c.r(), c.g(), c.b()).to_rgb888();
            self.set_pixel(Point::new(p.x, p.y));
        }
        self.color = pen;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pixel_writes_pen_colour() {
        let mut g = PenRgb888::<4, 3>::new();
        g.set_pen_rgb(1, 2, 3);
        g.set_pixel(Point::new(2, 1));
        assert_eq!(g.frame_buffer()[1][2], 0x010203);
        assert_eq!(g.frame_buffer()[0][0], 0);
    }

    #[test]
    fn out_of_bounds_is_ignored() {
        let mut g = PenRgb888::<4, 3>::new();
        g.set_pen(0xffffff);
        g.set_pixel(Point::new(-1, 0));
        g.set_pixel(Point::new(4, 0));
        g.set_pixel_span(Point::new(2, 2), 10);
        assert_eq!(g.frame_buffer()[2], [0, 0, 0xffffff, 0xffffff]);
    }

    #[test]
    fn clear_fills_everything() {
        let mut g = PenRgb888::<4, 3>::new();
        g.set_pen(7);
        g.clear();
        assert!(g.frame_buffer().iter().flatten().all(|&p| p == 7));
    }

    #[test]
    fn clip_limits_rectangle() {
        let mut g = PenRgb888::<4, 3>::new();
        g.set_clip(Rect::new(1, 1, 2, 1));
        g.set_pen(9);
        g.rectangle(Rect::new(0, 0, 4, 3));
        assert_eq!(g.frame_buffer()[1], [0, 9, 9, 0]);
        assert_eq!(g.frame_buffer()[0], [0; 4]);
    }

    #[test]
    fn draw_target_keeps_pen() {
        let mut g = PenRgb888::<4, 3>::new();
        g.set_pen(5);
        g.draw_iter([Pixel(embedded_graphics_core::geometry::Point::new(0, 0), Rgb888::new(1, 2, 3))])
            .unwrap();
        assert_eq!(g.frame_buffer()[0][0], 0x010203);
        g.set_pixel(Point::new(1, 0));
        assert_eq!(g.frame_buffer()[0][1], 5);
    }
}
