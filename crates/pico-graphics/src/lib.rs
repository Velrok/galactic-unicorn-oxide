//! Port of the subset of Pimoroni's `pico_graphics` the Galactic Unicorn needs.
//!
//! Pico Graphics is a tiny graphics library: a framebuffer, a current "pen",
//! and a few drawing calls. The C++ library supports many buffer formats
//! (`1Bit`, `3Bit`, `P4`, `P8`, `RGB332`, `RGB565`, `RGB888`); this port has
//! only [`PenRgb888`], which is what the Galactic Unicorn uses.
//!
//! Text and shapes are not reimplemented. [`PenRgb888`] implements
//! [`DrawTarget`](embedded_graphics_core::draw_target::DrawTarget), so anything from
//! [`embedded-graphics`](https://docs.rs/embedded-graphics) draws into it.
//!
//! # Example
//!
//! ```
//! use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_6X10};
//! use embedded_graphics::pixelcolor::Rgb888;
//! use embedded_graphics::prelude::*;
//! use embedded_graphics::text::Text;
//! use pico_graphics::{PenRgb888, Point, Rect};
//!
//! // 53x11 is the Galactic Unicorn's size.
//! let mut graphics = PenRgb888::<53, 11>::new();
//!
//! // Native calls: set the pen, then draw with it.
//! graphics.set_pen_rgb(0, 0, 0);
//! graphics.clear();
//! graphics.set_pen_rgb(255, 255, 0);
//! graphics.rectangle(Rect::new(0, 0, 3, 3));
//! graphics.set_pixel(Point::new(10, 5));
//!
//! // embedded-graphics for text and shapes.
//! let style = MonoTextStyle::new(&FONT_6X10, Rgb888::WHITE);
//! Text::new("Hi", embedded_graphics::geometry::Point::new(12, 8), style)
//!     .draw(&mut graphics)
//!     .unwrap();
//!
//! assert_eq!(graphics.frame_buffer()[0][0], 0xffff00);
//! ```
//!
//! # C++ to Rust
//!
//! See `docs/pico-graphics.md` in the repository for the full mapping.
//!
//! Reference: `libraries/pico_graphics` (`pico_graphics.hpp`, `pico_graphics_pen_rgb888.cpp`).
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]

mod geometry;
mod pen_rgb888;
mod rgb;

pub use geometry::{Point, Rect};
pub use pen_rgb888::PenRgb888;
pub use rgb::Rgb;
