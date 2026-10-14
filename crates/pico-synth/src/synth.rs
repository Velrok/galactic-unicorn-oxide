use crate::SAMPLE_RATE;
use crate::channel::{AdsrPhase, AudioChannel, Waveform};
use crate::tables::SINE_WAVEFORM;

/// Number of voices.
const CHANNEL_COUNT: usize = 8;

/// Eight voices mixed to one mono stream (`PicoSynth`).
pub struct PicoSynth {
    /// Master volume, `0..=65535`.
    pub volume: u16,
    /// The voices.
    pub channels: [AudioChannel; CHANNEL_COUNT],
    prng_state: u32,
}

impl PicoSynth {
    /// Number of voices (8).
    pub const CHANNEL_COUNT: usize = CHANNEL_COUNT;

    /// A silent synth with the C++ default master volume `0x2fff`.
    pub const fn new() -> Self {
        Self {
            volume: 0x2fff,
            channels: [AudioChannel::new(); CHANNEL_COUNT],
            prng_state: 0x32B71700,
        }
    }

    fn prng_next(&mut self) -> u32 {
        let mut x = self.prng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.prng_state = x;
        x
    }

    /// Rough approximation of a normal distribution.
    fn prng_normal(&mut self) -> i32 {
        let r0 = self.prng_next();
        let r1 = self.prng_next();
        let n = ((r0 & 0xffff) + (r1 & 0xffff) + (r0 >> 16) + (r1 >> 16)) / 2;
        n.wrapping_sub(0xffff) as i32
    }

    /// True while master volume is non-zero and any audible channel is not `Off`.
    pub fn is_audio_playing(&self) -> bool {
        self.volume != 0
            && self
                .channels
                .iter()
                .any(|c| c.volume > 0 && c.adsr_phase != AdsrPhase::Off)
    }

    /// Mix and return the next sample.
    pub fn get_audio_frame(&mut self) -> i16 {
        let mut sample: i32 = 0;

        for c in 0..CHANNEL_COUNT {
            // Advance the waveform position (16-bit fixed point).
            let ch = &mut self.channels[c];
            ch.waveform_offset = ch
                .waveform_offset
                .wrapping_add(((ch.frequency as u32 * 256) << 8) / SAMPLE_RATE);

            if ch.adsr_phase == AdsrPhase::Off {
                continue;
            }

            if ch.adsr_frame >= ch.adsr_end_frame && ch.adsr_phase != AdsrPhase::Sustain {
                match ch.adsr_phase {
                    AdsrPhase::Attack => ch.trigger_decay(),
                    AdsrPhase::Decay => ch.trigger_sustain(),
                    AdsrPhase::Release => ch.off(),
                    _ => {}
                }
            }

            ch.adsr_level = ch.adsr_level.wrapping_add(ch.adsr_step as u32);
            ch.adsr_frame += 1;

            // On overflow of the waveform position, draw a new noise sample.
            let new_noise = ch.waveform_offset & 0x10000 != 0;
            ch.waveform_offset &= 0xffff;
            if new_noise {
                let n = self.prng_normal();
                self.channels[c].noise = n as i16;
            }
            let ch = &mut self.channels[c];

            if ch.waveforms == 0 {
                continue;
            }

            let offset = ch.waveform_offset as i32;
            let mut count: i32 = 0;
            let mut channel_sample: i32 = 0;

            if ch.waveforms & Waveform::NOISE != 0 {
                channel_sample += ch.noise as i32;
                count += 1;
            }
            if ch.waveforms & Waveform::SAW != 0 {
                channel_sample += offset - 0x7fff;
                count += 1;
            }
            if ch.waveforms & Waveform::TRIANGLE != 0 {
                channel_sample += if offset < 0x7fff {
                    offset * 2 - 0x7fff
                } else {
                    0x7fff - (offset - 0x7fff) * 2
                };
                count += 1;
            }
            if ch.waveforms & Waveform::SQUARE != 0 {
                channel_sample += if (offset as u32) < ch.pulse_width as u32 { 0x7fff } else { -0x7fff };
                count += 1;
            }
            if ch.waveforms & Waveform::SINE != 0 {
                // 256 table entries: index with the top bits of the position.
                channel_sample += SINE_WAVEFORM[(offset >> 8) as usize] as i32;
                count += 1;
            }
            if ch.waveforms & Waveform::WAVE != 0 {
                channel_sample += ch.wave_buffer[ch.wave_buf_pos as usize] as i32;
                ch.wave_buf_pos += 1;
                if ch.wave_buf_pos == 64 {
                    ch.wave_buf_pos = 0;
                    if let Some(callback) = ch.wave_buffer_callback {
                        callback(ch);
                    }
                }
                count += 1;
            }

            channel_sample /= count;

            // Envelope, then channel volume.
            channel_sample = ((channel_sample as i64 * (ch.adsr_level >> 8) as i32 as i64) >> 16) as i32;
            channel_sample = ((channel_sample as i64 * ch.volume as i64) >> 16) as i32;

            sample += channel_sample;
        }

        sample = ((sample as i64 * self.volume as i64) >> 16) as i32;
        sample.clamp(-0x8000, 0x7fff) as i16
    }
}

impl Default for PicoSynth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud() -> PicoSynth {
        let mut s = PicoSynth::new();
        s.volume = 0xffff;
        s
    }

    #[test]
    fn silent_until_triggered() {
        let mut s = loud();
        s.channels[0].waveforms = Waveform::SINE;
        assert!(!s.is_audio_playing());
        assert!((0..100).all(|_| s.get_audio_frame() == 0));
    }

    #[test]
    fn trigger_starts_playing_and_release_ends_it() {
        let mut s = loud();
        let c = &mut s.channels[0];
        c.waveforms = Waveform::SQUARE;
        c.frequency = 440;
        c.trigger_attack();
        assert!(s.is_audio_playing());
        for _ in 0..2000 {
            s.get_audio_frame();
        }
        assert_eq!(s.channels[0].adsr_phase, AdsrPhase::Sustain);
        s.channels[0].trigger_release();
        for _ in 0..2000 {
            s.get_audio_frame();
        }
        assert_eq!(s.channels[0].adsr_phase, AdsrPhase::Off);
        assert!(!s.is_audio_playing());
    }

    #[test]
    fn envelope_phases_run_in_order() {
        let mut s = loud();
        let c = &mut s.channels[0];
        c.waveforms = Waveform::SINE;
        c.attack_ms = 10;
        c.decay_ms = 10;
        c.sustain = 0x8000;
        c.trigger_attack();
        let mut seen = std::vec::Vec::new();
        for _ in 0..1000 {
            s.get_audio_frame();
            let p = s.channels[0].adsr_phase;
            if seen.last() != Some(&p) {
                seen.push(p);
            }
        }
        assert_eq!(seen, [AdsrPhase::Attack, AdsrPhase::Decay, AdsrPhase::Sustain]);
    }

    #[test]
    fn sustain_level_is_reached() {
        let mut s = loud();
        let c = &mut s.channels[0];
        c.waveforms = Waveform::SINE;
        c.attack_ms = 5;
        c.decay_ms = 5;
        c.sustain = 0x8000;
        c.trigger_attack();
        for _ in 0..1000 {
            s.get_audio_frame();
        }
        let level = s.channels[0].adsr_level >> 8;
        assert!((0x7f00..=0x8100).contains(&level), "level {level:#x}");
    }

    #[test]
    fn zero_attack_does_not_divide_by_zero() {
        let mut c = AudioChannel::new();
        c.attack_ms = 0;
        c.trigger_attack();
        assert_eq!(c.adsr_step, 0);
    }

    #[test]
    fn square_wave_alternates_sign() {
        let mut s = loud();
        let c = &mut s.channels[0];
        c.waveforms = Waveform::SQUARE;
        c.frequency = 1000;
        c.attack_ms = 1;
        c.trigger_attack();
        let samples: std::vec::Vec<i16> = (0..220).map(|_| s.get_audio_frame()).collect();
        assert!(samples.iter().any(|&x| x > 1000));
        assert!(samples.iter().any(|&x| x < -1000));
    }

    #[test]
    fn output_is_clipped_not_wrapped() {
        let mut s = loud();
        for c in &mut s.channels {
            c.waveforms = Waveform::SQUARE;
            c.attack_ms = 1;
            c.trigger_attack();
        }
        for _ in 0..500 {
            let x = s.get_audio_frame();
            assert!((-0x8000..=0x7fff).contains(&(x as i32)));
        }
    }

    #[test]
    fn zero_master_volume_is_silent() {
        let mut s = PicoSynth::new();
        s.volume = 0;
        s.channels[0].waveforms = Waveform::SINE;
        s.channels[0].trigger_attack();
        assert!(!s.is_audio_playing());
        assert!((0..500).all(|_| s.get_audio_frame() == 0));
    }

    #[test]
    fn wave_buffer_callback_runs_after_64_samples() {
        fn refill(c: &mut AudioChannel) {
            c.user_data += 1;
        }
        let mut s = loud();
        let c = &mut s.channels[0];
        c.waveforms = Waveform::WAVE;
        c.wave_buffer_callback = Some(refill);
        c.attack_ms = 1;
        c.trigger_attack();
        for _ in 0..128 {
            s.get_audio_frame();
        }
        assert_eq!(s.channels[0].user_data, 2);
    }

    #[test]
    fn noise_is_deterministic() {
        let run = || {
            let mut s = loud();
            s.channels[0].waveforms = Waveform::NOISE;
            s.channels[0].frequency = 4000;
            s.channels[0].trigger_attack();
            (0..300).map(|_| s.get_audio_frame()).collect::<std::vec::Vec<_>>()
        };
        assert_eq!(run(), run());
    }
}
