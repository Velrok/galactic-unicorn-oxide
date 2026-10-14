//! The buffer the PIO consumes, via a looping DMA transfer.
//!
//! The PIO cannot access memory at random and has almost no branching, so the data
//! is "interleaved": delivered in exactly the order the PIO program needs it, with
//! the row, pixel count and timing values inserted between pixels.
//!
//! ```text
//! for each row:
//!   for each bcd frame:
//!        0: pixel count - 1
//!        1: row select
//!   2..=54: xxxxxrgb pixels (bit 0 blue, 1 green, 2 red)
//!       55: dummy byte (dword alignment)
//!   56..=59: bcd tick count, little endian
//! ```

/// Display width in pixels.
pub const WIDTH: usize = 53;
/// Display height in pixels.
pub const HEIGHT: usize = 11;

/// Rows scanned one at a time.
pub const ROW_COUNT: usize = HEIGHT;
/// Binary-coded-modulation frames per row: one per bit of the 14-bit gamma output.
pub const BCD_FRAME_COUNT: usize = 14;
/// Bytes in one BCD frame (header, 53 pixels, padding, tick count).
pub const BCD_FRAME_BYTES: usize = 60;
/// Bytes in one row: all of its BCD frames.
pub const ROW_BYTES: usize = BCD_FRAME_COUNT * BCD_FRAME_BYTES;
/// Total bitstream size: 11 rows x 14 frames x 60 bytes = 9,240.
pub const BITSTREAM_LENGTH: usize = ROW_COUNT * ROW_BYTES;

/// 32-bit aligned so DMA can move it a word at a time.
#[repr(C, align(4))]
/// The interleaved framebuffer. Build with [`Bitstream::new`], write with
/// [`Bitstream::set_pixel`].
pub struct Bitstream(pub [u8; BITSTREAM_LENGTH]);

impl Bitstream {
    /// All pixels off, with row selects and BCD timings filled in.
    ///
    /// ```
    /// use galactic_unicorn::bitstream::{Bitstream, BITSTREAM_LENGTH};
    /// let mut bs = Bitstream::new();
    /// assert_eq!(bs.0.len(), BITSTREAM_LENGTH);
    /// bs.set_pixel(0, 0, 255, 255, 255, 256); // white, full brightness
    /// ```
    pub const fn new() -> Self {
        let mut buf = [0u8; BITSTREAM_LENGTH];
        let mut row = 0;
        while row < HEIGHT {
            let mut frame = 0;
            while frame < BCD_FRAME_COUNT {
                let p = row * ROW_BYTES + BCD_FRAME_BYTES * frame;
                buf[p] = (WIDTH - 1) as u8;
                buf[p + 1] = row as u8;
                let ticks = (1u32 << frame).to_le_bytes();
                buf[p + 56] = ticks[0];
                buf[p + 57] = ticks[1];
                buf[p + 58] = ticks[2];
                buf[p + 59] = ticks[3];
                frame += 1;
            }
            row += 1;
        }
        Self(buf)
    }

    /// Write one pixel into every BCD frame.
    ///
    /// `brightness` is `0..=256`. Coordinates are flipped to match how the
    /// panel is wired; out-of-range coordinates are ignored.
    pub fn set_pixel(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8, brightness: u16) {
        if x < 0 || x >= WIDTH as i32 || y < 0 || y >= HEIGHT as i32 {
            return;
        }
        let x = WIDTH - 1 - x as usize;
        let y = HEIGHT - 1 - y as usize;

        let scale = |c: u8| ((c as u16 * brightness) >> 8) as usize;
        let mut gamma_r = GAMMA_14BIT[scale(r)];
        let mut gamma_g = GAMMA_14BIT[scale(g)];
        let mut gamma_b = GAMMA_14BIT[scale(b)];

        for frame in 0..BCD_FRAME_COUNT {
            let i = y * ROW_BYTES + BCD_FRAME_BYTES * frame + 2 + x;
            let v = (gamma_b & 1) | (gamma_g & 1) << 1 | (gamma_r & 1) << 2;
            // The DMA engine reads this buffer concurrently.
            unsafe { core::ptr::write_volatile(&mut self.0[i], v as u8) };
            gamma_r >>= 1;
            gamma_g >>= 1;
            gamma_b >>= 1;
        }
    }
}

impl Default for Bitstream {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps an 8-bit channel value to the 14 bits shown over the BCD frames:
/// `(n / 255) ^ 2.2 * 16383 + 0.5`.
pub const GAMMA_14BIT: [u16; 256] = [
          0, 0, 0, 1, 2, 3, 4, 6, 8, 10, 13, 16, 20, 23, 28, 32,
          37, 42, 48, 54, 61, 67, 75, 82, 90, 99, 108, 117, 127, 137, 148, 159,
          170, 182, 195, 207, 221, 234, 249, 263, 278, 294, 310, 326, 343, 361, 379, 397,
          416, 435, 455, 475, 496, 517, 539, 561, 583, 607, 630, 654, 679, 704, 730, 756,
          783, 810, 838, 866, 894, 924, 953, 983, 1014, 1045, 1077, 1110, 1142, 1176, 1210, 1244,
          1279, 1314, 1350, 1387, 1424, 1461, 1499, 1538, 1577, 1617, 1657, 1698, 1739, 1781, 1823, 1866,
          1910, 1954, 1998, 2044, 2089, 2136, 2182, 2230, 2278, 2326, 2375, 2425, 2475, 2525, 2577, 2629,
          2681, 2734, 2787, 2841, 2896, 2951, 3007, 3063, 3120, 3178, 3236, 3295, 3354, 3414, 3474, 3535,
          3596, 3658, 3721, 3784, 3848, 3913, 3978, 4043, 4110, 4176, 4244, 4312, 4380, 4449, 4519, 4589,
          4660, 4732, 4804, 4876, 4950, 5024, 5098, 5173, 5249, 5325, 5402, 5479, 5557, 5636, 5715, 5795,
          5876, 5957, 6039, 6121, 6204, 6287, 6372, 6456, 6542, 6628, 6714, 6801, 6889, 6978, 7067, 7156,
          7247, 7337, 7429, 7521, 7614, 7707, 7801, 7896, 7991, 8087, 8183, 8281, 8378, 8477, 8576, 8675,
          8775, 8876, 8978, 9080, 9183, 9286, 9390, 9495, 9600, 9706, 9812, 9920, 10027, 10136, 10245, 10355,
          10465, 10576, 10688, 10800, 10913, 11027, 11141, 11256, 11371, 11487, 11604, 11721, 11840, 11958, 12078, 12198,
          12318, 12440, 12562, 12684, 12807, 12931, 13056, 13181, 13307, 13433, 13561, 13688, 13817, 13946, 14076, 14206,
          14337, 14469, 14602, 14735, 14868, 15003, 15138, 15273, 15410, 15547, 15685, 15823, 15962, 16102, 16242, 16383
];

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel_bits(bs: &Bitstream, x: usize, y: usize) -> [u8; BCD_FRAME_COUNT] {
        core::array::from_fn(|f| bs.0[y * ROW_BYTES + f * BCD_FRAME_BYTES + 2 + x])
    }

    #[test]
    fn length_matches_docs() {
        assert_eq!(BITSTREAM_LENGTH, 9240);
        assert_eq!(BITSTREAM_LENGTH % 4, 0);
    }

    #[test]
    fn header_and_ticks_are_filled_in() {
        let bs = Bitstream::new();
        for row in 0..HEIGHT {
            for frame in 0..BCD_FRAME_COUNT {
                let f = &bs.0[row * ROW_BYTES + frame * BCD_FRAME_BYTES..][..BCD_FRAME_BYTES];
                assert_eq!(f[0], 52);
                assert_eq!(f[1], row as u8);
                assert_eq!(u32::from_le_bytes([f[56], f[57], f[58], f[59]]), 1 << frame);
            }
        }
    }

    #[test]
    fn gamma_table_ends_are_right() {
        assert_eq!(GAMMA_14BIT[0], 0);
        assert_eq!(GAMMA_14BIT[255], 16383);
        assert!(GAMMA_14BIT.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn full_white_sets_every_frame_bit_for_each_channel() {
        let mut bs = Bitstream::new();
        bs.set_pixel(0, 0, 255, 255, 255, 256);
        // (0,0) lands at the flipped corner
        assert_eq!(pixel_bits(&bs, WIDTH - 1, HEIGHT - 1), [0b111; BCD_FRAME_COUNT]);
    }

    #[test]
    fn channels_map_to_bits() {
        let mut bs = Bitstream::new();
        bs.set_pixel(0, 0, 255, 0, 0, 256);
        assert!(pixel_bits(&bs, WIDTH - 1, HEIGHT - 1).iter().all(|&b| b == 0b100));
        bs.set_pixel(0, 0, 0, 255, 0, 256);
        assert!(pixel_bits(&bs, WIDTH - 1, HEIGHT - 1).iter().all(|&b| b == 0b010));
        bs.set_pixel(0, 0, 0, 0, 255, 256);
        assert!(pixel_bits(&bs, WIDTH - 1, HEIGHT - 1).iter().all(|&b| b == 0b001));
    }

    #[test]
    fn zero_brightness_is_black() {
        let mut bs = Bitstream::new();
        bs.set_pixel(3, 3, 255, 255, 255, 0);
        assert_eq!(pixel_bits(&bs, WIDTH - 1 - 3, HEIGHT - 1 - 3), [0; BCD_FRAME_COUNT]);
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut bs = Bitstream::new();
        let before = bs.0;
        bs.set_pixel(-1, 0, 255, 255, 255, 256);
        bs.set_pixel(WIDTH as i32, 0, 255, 255, 255, 256);
        bs.set_pixel(0, HEIGHT as i32, 255, 255, 255, 256);
        assert_eq!(bs.0, before);
    }
}
