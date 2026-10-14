/// 8-bit-per-channel colour (`pimoroni::RGB`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Unpack `0x00RRGGBB`.
    pub const fn from_rgb888(c: u32) -> Self {
        Self::new((c >> 16) as u8, (c >> 8) as u8, c as u8)
    }

    /// Pack as `0x00RRGGBB`.
    pub const fn to_rgb888(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }

    /// `h`, `s`, `v` in `0.0..=1.0`.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h6 = h * 6.0;
        let i = h6 as i32; // truncation == floor for h >= 0
        let f = h6 - i as f32;
        let v = v * 255.0;
        let p = (v * (1.0 - s)) as u8;
        let q = (v * (1.0 - f * s)) as u8;
        let t = (v * (1.0 - (1.0 - f) * s)) as u8;
        let v = v as u8;
        match i.rem_euclid(6) {
            0 => Self::new(v, t, p),
            1 => Self::new(q, v, p),
            2 => Self::new(p, v, t),
            3 => Self::new(p, q, v),
            4 => Self::new(t, p, v),
            _ => Self::new(v, p, q),
        }
    }

    /// Blend `with` over `self`, `alpha` 0 = all `self`, 255 = ~all `with`.
    pub fn blend(self, with: Rgb, alpha: u8) -> Self {
        let mix = |d: u8, s: u8| (d as i32 + ((alpha as i32 * (s as i32 - d as i32) + 127) >> 8)) as u8;
        Self::new(mix(self.r, with.r), mix(self.g, with.g), mix(self.b, with.b))
    }

    /// Rough perceived brightness, for comparing colours.
    pub fn luminance(self) -> i32 {
        self.r as i32 * 21 + self.g as i32 * 72 + self.b as i32 * 7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb888_round_trips() {
        let c = Rgb::new(0x12, 0x34, 0x56);
        assert_eq!(c.to_rgb888(), 0x123456);
        assert_eq!(Rgb::from_rgb888(0x123456), c);
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(Rgb::from_hsv(0.0, 1.0, 1.0), Rgb::new(255, 0, 0));
        assert_eq!(Rgb::from_hsv(1.0 / 3.0 + 0.001, 1.0, 1.0).g, 255);
    }

    #[test]
    fn blend_extremes() {
        let a = Rgb::new(0, 0, 0);
        let b = Rgb::new(200, 100, 50);
        assert_eq!(a.blend(b, 0), a);
        assert!(a.blend(b, 255).r >= 198);
    }
}
