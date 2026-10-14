//! Port of Pimoroni's `galactic_unicorn` driver for the RP2350 (Pico 2 W).
//!
//! The Galactic Unicorn has 53x11 RGB LEDs driven by the PIO. The driver applies
//! gamma correction and refreshes with 14-bit precision (binary-coded modulation),
//! with no CPU involved once started: a looping DMA transfer feeds a PIO program.
//!
//! - [`bitstream`] is hardware independent: it builds the interleaved buffer the PIO
//!   shifts out, and is unit-tested on the host.
//! - `GalacticUnicorn` (bare-metal only) owns the PIO state machine, the DMA, the
//!   buttons and the light sensor.
//!
//! Draw with [`pico_graphics::PenRgb888`] (or any `embedded-graphics` drawing, via its
//! `DrawTarget` impl), then pass it to [`GalacticUnicorn::update`].
//!
//! # Example
//!
//! ```ignore
//! use embassy_rp::bind_interrupts;
//! use embassy_rp::peripherals::PIO0;
//! use embassy_rp::pio::{self, Pio};
//! use galactic_unicorn::{GalacticUnicorn, HEIGHT, WIDTH};
//! use pico_graphics::{PenRgb888, Point};
//!
//! bind_interrupts!(struct Irqs {
//!     PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
//! });
//!
//! let p = embassy_rp::init(Default::default());
//! let pio = Pio::new(p.PIO0, Irqs);
//! let mut unicorn = GalacticUnicorn::new(galactic_unicorn::peripherals!(p, pio));
//!
//! let mut graphics = PenRgb888::<WIDTH, HEIGHT>::new();
//! graphics.set_pen_rgb(255, 255, 0);
//! graphics.set_pixel(Point::new(10, 5));
//! unicorn.update(&graphics);
//! ```
//!
//! # Not ported
//!
//! Audio (`play_sample`, the synth, volume, the MUTE pin) and the teardown /
//! re-`init` logic. See `docs/galactic-unicorn.md` in the repository.
//!
//! Reference: `libraries/galactic_unicorn` (`galactic_unicorn.cpp`, `.hpp`, `.pio`).
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]

pub mod bitstream;
#[cfg(target_os = "none")]
mod driver;

#[cfg(target_os = "none")]
pub use driver::{Button, GalacticUnicorn, Peripherals};

pub use bitstream::{HEIGHT, WIDTH};
