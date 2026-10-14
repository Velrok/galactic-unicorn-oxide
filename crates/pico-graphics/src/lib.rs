//! Port of the subset of Pimoroni's `pico_graphics` the Galactic Unicorn needs.
//!
//! Only the RGB888 pen is ported. Text and shapes beyond the basics come from
//! `embedded-graphics` via the [`DrawTarget`](embedded_graphics_core::draw_target::DrawTarget)
//! impl on [`PenRgb888`].
//!
//! Reference: `libraries/pico_graphics` (`pico_graphics.hpp`, `pico_graphics_pen_rgb888.cpp`).
#![cfg_attr(not(test), no_std)]

mod geometry;
mod pen_rgb888;
mod rgb;

pub use geometry::{Point, Rect};
pub use pen_rgb888::PenRgb888;
pub use rgb::Rgb;
