//! Audio output: I2S out of a second PIO state machine, fed by DMA.
//!
//! Two sources, as in the C++ driver:
//!
//! - **Samples:** one DMA transfer straight from your buffer.
//! - **Synth:** four-sample double buffers. When a transfer finishes, the DMA interrupt
//!   starts the next buffer and has the synth fill the one that just played.
//!
//! The synth is shared with that interrupt, so it lives behind a critical section;
//! reach it through `GalacticUnicorn::with_synth`.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU8, Ordering, compiler_fence};

use critical_section::{CriticalSection, Mutex};
use embassy_rp::interrupt::typelevel::{DMA_IRQ_0, Handler};
use embassy_rp::pac;
use embassy_rp::pac::dma::regs::{ChanAbort, CtrlTrig};
use embassy_rp::pac::dma::vals::{DataSize, TreqSel};
use pico_synth::PicoSynth;

/// Samples per synth buffer.
pub(crate) const TONE_BUFFER_SIZE: usize = 4;
/// Synth buffers, played alternately.
pub(crate) const NUM_TONE_BUFFERS: usize = 2;

/// DMA request number of PIO0 TX FIFO of state machine 1.
const PIO0_TX1_DREQ: u8 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayMode {
    PlayingBuffer,
    PlayingSynth,
    NotPlaying,
}

pub(crate) struct Audio {
    pub mode: PlayMode,
    current_buffer: usize,
    tone_buffers: [[i16; TONE_BUFFER_SIZE]; NUM_TONE_BUFFERS],
    pub synth: PicoSynth,
}

pub(crate) static AUDIO: Mutex<RefCell<Audio>> = Mutex::new(RefCell::new(Audio {
    mode: PlayMode::NotPlaying,
    current_buffer: 0,
    tone_buffers: [[0; TONE_BUFFER_SIZE]; NUM_TONE_BUFFERS],
    synth: PicoSynth::new(),
}));

/// Number of the DMA channel that feeds the I2S state machine; `NONE` until set up.
static DMA_CHANNEL: AtomicU8 = AtomicU8::new(NONE);
const NONE: u8 = 0xff;

/// Atomically set bits in a peripheral register (RP2350 SET alias, +0x2000).
fn set_bits(reg: *mut u32, bits: u32) {
    unsafe { core::ptr::write_volatile((reg as usize + 0x2000) as *mut u32, bits) };
}

/// Atomically clear bits in a peripheral register (RP2350 CLR alias, +0x3000).
fn clear_bits(reg: *mut u32, bits: u32) {
    unsafe { core::ptr::write_volatile((reg as usize + 0x3000) as *mut u32, bits) };
}

pub(crate) fn set_dma_channel(channel: u8) {
    DMA_CHANNEL.store(channel, Ordering::Relaxed);
    set_bits(pac::DMA.inte(0).as_ptr(), 1 << channel);
}

fn channel() -> pac::dma::Channel {
    pac::DMA.ch(DMA_CHANNEL.load(Ordering::Relaxed) as usize)
}

/// Start a 16-bit transfer of `count` samples from `data` into the I2S FIFO.
pub(crate) fn start_transfer(data: *const i16, count: u32) {
    let ch = channel();
    ch.read_addr().write_value(data as u32);
    ch.write_addr().write_value(pac::PIO0.txf(1).as_ptr() as u32);
    ch.trans_count().write(|w| {
        w.set_mode(0.into());
        w.set_count(count);
    });
    compiler_fence(Ordering::SeqCst);
    let mut c = CtrlTrig(0);
    c.set_treq_sel(TreqSel::from(PIO0_TX1_DREQ));
    c.set_data_size(DataSize::SIZE_HALFWORD);
    c.set_incr_read(true);
    c.set_incr_write(false);
    c.set_chain_to(DMA_CHANNEL.load(Ordering::Relaxed)); // chained to itself: no chaining
    c.set_en(true);
    ch.ctrl_trig().write_value(c); // writing CTRL_TRIG starts the transfer
    compiler_fence(Ordering::SeqCst);
}

/// Abort the audio transfer without leaving a stale interrupt behind.
///
/// Ported from the pico-sdk fix the C++ driver copies (`dma_safe_abort`).
pub(crate) fn dma_safe_abort() {
    let channel_number = DMA_CHANNEL.load(Ordering::Relaxed);
    let mask = 1u32 << channel_number;
    let irq_enabled = pac::DMA.inte(0).read() & mask;
    clear_bits(pac::DMA.inte(0).as_ptr(), irq_enabled);

    pac::DMA.chan_abort().write_value(ChanAbort(mask));
    // Poll BUSY rather than the ABORT bit, which can clear prematurely.
    while channel().ctrl_trig().read().busy() {}

    pac::DMA.ints(0).write_value(mask);
    set_bits(pac::DMA.inte(0).as_ptr(), irq_enabled);
}

impl Audio {
    /// Start from the first buffer.
    pub(crate) fn reset_buffers(&mut self) {
        self.current_buffer = 0;
    }

    /// Fill the next synth buffer.
    pub(crate) fn populate_next_synth(&mut self) {
        let i = self.current_buffer;
        for s in 0..TONE_BUFFER_SIZE {
            self.tone_buffers[i][s] = self.synth.get_audio_frame();
        }
    }
}

/// Called when a transfer finishes: queue the next synth buffer, or go idle.
pub(crate) fn next_audio_sequence(cs: CriticalSection<'_>) {
    // Clear the interrupt request caused by our channel.
    pac::DMA.ints(0).write_value(1 << DMA_CHANNEL.load(Ordering::Relaxed));

    let mut audio = AUDIO.borrow_ref_mut(cs);
    if audio.mode == PlayMode::PlayingSynth {
        let playing = audio.current_buffer;
        start_transfer(audio.tone_buffers[playing].as_ptr(), TONE_BUFFER_SIZE as u32);
        audio.current_buffer = (playing + 1) % NUM_TONE_BUFFERS;
        audio.populate_next_synth();
    } else {
        audio.mode = PlayMode::NotPlaying;
    }
}

/// DMA interrupt handler for the audio channel.
///
/// Bind it to `DMA_IRQ_0` and pass the binding to `GalacticUnicorn::new`:
///
/// ```ignore
/// bind_interrupts!(struct Irqs {
///     DMA_IRQ_0 => galactic_unicorn::AudioInterruptHandler;
/// });
/// ```
///
/// Other users of `DMA_IRQ_0` (such as embassy's own DMA driver) can share the
/// interrupt: list several handlers in `bind_interrupts!`.
pub struct AudioInterruptHandler;

impl Handler<DMA_IRQ_0> for AudioInterruptHandler {
    unsafe fn on_interrupt() {
        let channel_number = DMA_CHANNEL.load(Ordering::Relaxed);
        if channel_number != NONE && pac::DMA.ints(0).read() & (1 << channel_number) != 0 {
            critical_section::with(next_audio_sequence);
        }
    }
}
