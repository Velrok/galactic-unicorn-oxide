use crate::SAMPLE_RATE;

/// Waveform bits for [`AudioChannel::waveforms`]. Combine with `|`; enabled waveforms
/// are averaged.
pub struct Waveform;

impl Waveform {
    /// Random noise, refreshed at the voice frequency.
    pub const NOISE: u8 = 128;
    /// Square wave; see [`AudioChannel::pulse_width`].
    pub const SQUARE: u8 = 64;
    /// Sawtooth.
    pub const SAW: u8 = 32;
    /// Triangle.
    pub const TRIANGLE: u8 = 16;
    /// Sine, from a 256-entry table.
    pub const SINE: u8 = 8;
    /// The channel's 64-sample [`wave_buffer`](AudioChannel::wave_buffer).
    pub const WAVE: u8 = 1;
}

/// Where a channel is in its envelope (`ADSRPhase`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdsrPhase {
    /// Rising to full volume.
    Attack,
    /// Falling to the sustain level.
    Decay,
    /// Holding the sustain level until released.
    Sustain,
    /// Fading to zero.
    Release,
    /// Silent.
    Off,
}

/// One synth voice (`AudioChannel`).
///
/// Configure the public fields, then call [`trigger_attack`](Self::trigger_attack).
#[derive(Clone, Copy, Debug)]
pub struct AudioChannel {
    /// Enabled waveforms, a bitmask of [`Waveform`] constants.
    pub waveforms: u8,
    /// Voice frequency in Hz.
    pub frequency: u16,
    /// Channel volume, `0..=65535`.
    pub volume: u16,

    /// Attack time in ms. Zero is treated as an instant attack (the C++ divides by zero).
    pub attack_ms: u16,
    /// Decay time in ms. Zero is treated as an instant decay.
    pub decay_ms: u16,
    /// Sustain level, `0..=65535`.
    pub sustain: u16,
    /// Release time in ms. Zero is treated as an instant release.
    pub release_ms: u16,
    /// Square wave duty cycle, `0..=65535` (default `0x7fff`, 50%).
    pub pulse_width: u16,
    /// Current noise value.
    pub noise: i16,

    /// Position in the waveform, 16-bit fixed point (`0..=0xffff`).
    pub waveform_offset: u32,

    /// Unused in the C++ too (the filter is commented out); kept for parity.
    pub filter_last_sample: i32,
    /// Unused in the C++ too; kept for parity.
    pub filter_enable: bool,
    /// Unused in the C++ too; kept for parity.
    pub filter_cutoff_frequency: u16,

    /// Frames into the current envelope phase.
    pub adsr_frame: u32,
    /// Frame at which the envelope moves to the next phase.
    pub adsr_end_frame: u32,
    /// Current envelope level (24-bit; `0xffffff` is full).
    pub adsr_level: u32,
    /// Amount added to the level each frame.
    pub adsr_step: i32,
    /// Current envelope phase.
    pub adsr_phase: AdsrPhase,

    /// Read position in [`wave_buffer`](Self::wave_buffer).
    pub wave_buf_pos: u8,
    /// Samples for [`Waveform::WAVE`]. Small, because it is refilled by the callback.
    pub wave_buffer: [i16; 64],

    /// Free for the callback to use (`void *user_data` in C++).
    pub user_data: usize,
    /// Called each time `wave_buffer` has been played through, so it can be refilled.
    pub wave_buffer_callback: Option<fn(&mut AudioChannel)>,
}

impl AudioChannel {
    /// A silent channel with the C++ defaults.
    pub const fn new() -> Self {
        Self {
            waveforms: 0,
            frequency: 660,
            volume: u16::MAX,
            attack_ms: 2,
            decay_ms: 6,
            sustain: u16::MAX,
            release_ms: 1,
            pulse_width: 0x7fff,
            noise: 0,
            waveform_offset: 0,
            filter_last_sample: 0,
            filter_enable: false,
            filter_cutoff_frequency: 0,
            adsr_frame: 0,
            adsr_end_frame: 0,
            adsr_level: 0,
            adsr_step: 0,
            adsr_phase: AdsrPhase::Off,
            wave_buf_pos: 0,
            wave_buffer: [0; 64],
            user_data: 0,
            wave_buffer_callback: None,
        }
    }

    fn frames(ms: u16) -> u32 {
        (ms as u32 * SAMPLE_RATE) / 1000
    }

    /// Step per frame to move from the current level to `target` in `end_frame` frames.
    fn step_to(&self, target: i32, end_frame: u32) -> i32 {
        (target - self.adsr_level as i32)
            .checked_div(end_frame as i32)
            .unwrap_or(0)
    }

    /// Start the note: rise from the current level to full volume.
    pub fn trigger_attack(&mut self) {
        self.adsr_frame = 0;
        self.adsr_phase = AdsrPhase::Attack;
        self.adsr_end_frame = Self::frames(self.attack_ms);
        self.adsr_step = self.step_to(0xffffff, self.adsr_end_frame);
    }

    /// Fall from the current level to the sustain level.
    pub fn trigger_decay(&mut self) {
        self.adsr_frame = 0;
        self.adsr_phase = AdsrPhase::Decay;
        self.adsr_end_frame = Self::frames(self.decay_ms);
        self.adsr_step = self.step_to((self.sustain as i32) << 8, self.adsr_end_frame);
    }

    /// Hold the current level.
    pub fn trigger_sustain(&mut self) {
        self.adsr_frame = 0;
        self.adsr_phase = AdsrPhase::Sustain;
        self.adsr_end_frame = 0;
        self.adsr_step = 0;
    }

    /// End the note: fade from the current level to zero.
    pub fn trigger_release(&mut self) {
        self.adsr_frame = 0;
        self.adsr_phase = AdsrPhase::Release;
        self.adsr_end_frame = Self::frames(self.release_ms);
        self.adsr_step = self.step_to(0, self.adsr_end_frame);
    }

    /// Silence the channel immediately.
    pub fn off(&mut self) {
        self.adsr_frame = 0;
        self.adsr_phase = AdsrPhase::Off;
        self.adsr_step = 0;
    }

    /// Put the voice parameters back to their defaults (`restore`).
    pub fn restore(&mut self) {
        self.waveforms = 0;
        self.frequency = 660;
        self.volume = 0xffff;
        self.attack_ms = 2;
        self.decay_ms = 6;
        self.sustain = 0xffff;
        self.release_ms = 1;
        self.pulse_width = 0x7fff;
        self.noise = 0;
    }
}

impl Default for AudioChannel {
    fn default() -> Self {
        Self::new()
    }
}
