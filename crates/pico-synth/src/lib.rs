//! Port of Pimoroni's `pico_synth`: 8 voices with ADSR envelopes, similar in spirit to
//! the Commodore 64's SID chip.
//!
//! The synth produces mono 16-bit samples at [`SAMPLE_RATE`] (22,050 Hz), one per call
//! to [`PicoSynth::get_audio_frame`]. It has no hardware dependencies; the
//! `galactic-unicorn` crate pulls frames from it and feeds them to the I2S amplifier.
//!
//! # Envelope
//!
//! A note's shape is set by [`AudioChannel::attack_ms`], [`decay_ms`](AudioChannel::decay_ms),
//! [`sustain`](AudioChannel::sustain) and [`release_ms`](AudioChannel::release_ms):
//!
//! ```text
//! Off -> Attack -> Decay -> Sustain -> Release -> Off
//!       (to full) (to sustain) (held)   (to zero)
//! ```
//!
//! - **Attack:** milliseconds to reach full volume
//! - **Decay:** milliseconds to settle to the sustain level
//! - **Sustain:** the level held (`0..=65535`) until you call `trigger_release`
//! - **Release:** milliseconds to fade to zero after the note ends
//!
//! # Example
//!
//! ```
//! use pico_synth::{PicoSynth, Waveform};
//!
//! let mut synth = PicoSynth::new();
//! synth.volume = 0x8000;
//!
//! let channel = &mut synth.channels[0];
//! channel.waveforms = Waveform::SINE;
//! channel.frequency = 440;
//! channel.attack_ms = 10;
//! channel.decay_ms = 100;
//! channel.sustain = 0xafff;
//! channel.release_ms = 100;
//! channel.trigger_attack();
//!
//! assert!(synth.is_audio_playing());
//! let samples: Vec<i16> = (0..2205).map(|_| synth.get_audio_frame()).collect();
//! assert!(samples.iter().any(|&s| s != 0));
//! ```
//!
//! Reference: `libraries/pico_synth` (`pico_synth.hpp`, `pico_synth.cpp`).
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]

#[cfg(test)]
extern crate std;

mod channel;
mod synth;
mod tables;

pub use channel::{AudioChannel, AdsrPhase, Waveform};
pub use synth::PicoSynth;
pub use tables::SINE_WAVEFORM;

/// Output sample rate in Hz.
pub const SAMPLE_RATE: u32 = 22_050;
