//! Port of Pimoroni's `galactic_unicorn` driver for the RP2350 (Pico 2 W).
//!
//! - [`bitstream`] is hardware independent: it builds the BCD bitstream the
//!   PIO program shifts out, and is unit-tested on the host.
//! - `driver` (bare-metal only) owns the PIO state machine, the looping DMA,
//!   the buttons and the light sensor.
//!
//! Audio is not ported yet.
//!
//! Reference: `libraries/galactic_unicorn` (`galactic_unicorn.cpp`, `.hpp`, `.pio`).
#![cfg_attr(not(test), no_std)]

pub mod bitstream;
#[cfg(target_os = "none")]
mod driver;

#[cfg(target_os = "none")]
pub use driver::{Button, GalacticUnicorn, Peripherals};

pub use bitstream::{HEIGHT, WIDTH};
