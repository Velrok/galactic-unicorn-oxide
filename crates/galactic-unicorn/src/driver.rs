//! Hardware side of the driver: PIO state machine + looping DMA.
//!
//! Pin map and flow mirror `galactic_unicorn.cpp`:
//!
//! ```text
//! PIO side-set  13        column clock
//! PIO set       14-16     column data, latch, blank
//! PIO out       17-20     row select
//! ```
//!
//! Two chained DMA channels loop the bitstream into the PIO TX FIFO forever:
//! the control channel reloads the data channel's read address, the data
//! channel streams the buffer and chains back to the control channel.

use core::sync::atomic::{AtomicU32, Ordering, compiler_fence};

use embassy_rp::Peri;
use embassy_rp::adc::{Adc, Channel as AdcChannel, Config as AdcConfig};
use embassy_rp::dma::ChannelInstance;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::pac;
use embassy_rp::pac::dma::regs::CtrlTrig;
use embassy_rp::pac::dma::vals::{DataSize, TreqSel};
use embassy_rp::peripherals::*;
use embassy_rp::pio::{Common, Config, Direction, FifoJoin, Pio, ShiftConfig, ShiftDirection, StateMachine};
use embassy_time::{Duration, block_for};
use pico_graphics::PenRgb888;
use static_cell::StaticCell;

use crate::bitstream::{BITSTREAM_LENGTH, Bitstream, HEIGHT, WIDTH};

/// Driver-chip configuration register 1: full output current.
const REG1: u16 = 0b1111111111001110;

/// The control DMA channel copies this word into the data channel's read address.
static BITSTREAM_ADDR: AtomicU32 = AtomicU32::new(0);
static BITSTREAM: StaticCell<Bitstream> = StaticCell::new();

/// Front-panel buttons (`SWITCH_*` constants in C++), wired active-low with pull-ups.
///
/// Brightness, sleep and volume buttons are not tied to any hardware function; the
/// firmware decides what they do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Switch A, GPIO 0.
    A,
    /// Switch B, GPIO 1.
    B,
    /// Switch C, GPIO 3.
    C,
    /// Switch D, GPIO 6.
    D,
    /// Sleep, GPIO 27.
    Sleep,
    /// Volume up, GPIO 7.
    VolumeUp,
    /// Volume down, GPIO 8.
    VolumeDown,
    /// Brightness up, GPIO 21.
    BrightnessUp,
    /// Brightness down, GPIO 26.
    BrightnessDown,
}

/// Everything the driver takes ownership of.
///
/// Build it with [`peripherals!`](crate::peripherals) from `embassy_rp::init`'s result.
pub struct Peripherals<'d> {
    /// PIO0 with its interrupt bound; state machine 0 runs the display program.
    pub pio: Pio<'d, PIO0>,
    /// DMA channel that streams the bitstream into the PIO.
    pub dma_data: Peri<'d, DMA_CH0>,
    /// DMA channel that reloads `dma_data`'s read address so the stream loops.
    pub dma_ctrl: Peri<'d, DMA_CH1>,
    /// ADC, for the light sensor.
    pub adc: Peri<'d, ADC>,
    /// Column driver clock (PIO side-set).
    pub column_clock: Peri<'d, PIN_13>,
    /// Column driver data.
    pub column_data: Peri<'d, PIN_14>,
    /// Column driver latch.
    pub column_latch: Peri<'d, PIN_15>,
    /// Column driver blank (output disable).
    pub column_blank: Peri<'d, PIN_16>,
    /// Row select bit 0.
    pub row_bit_0: Peri<'d, PIN_17>,
    /// Row select bit 1.
    pub row_bit_1: Peri<'d, PIN_18>,
    /// Row select bit 2.
    pub row_bit_2: Peri<'d, PIN_19>,
    /// Row select bit 3.
    pub row_bit_3: Peri<'d, PIN_20>,
    /// Ambient light sensor (ADC input).
    pub light_sensor: Peri<'d, PIN_28>,
    /// See [`Button::A`].
    pub switch_a: Peri<'d, PIN_0>,
    /// See [`Button::B`].
    pub switch_b: Peri<'d, PIN_1>,
    /// See [`Button::C`].
    pub switch_c: Peri<'d, PIN_3>,
    /// See [`Button::D`].
    pub switch_d: Peri<'d, PIN_6>,
    /// See [`Button::Sleep`].
    pub switch_sleep: Peri<'d, PIN_27>,
    /// See [`Button::VolumeUp`].
    pub switch_volume_up: Peri<'d, PIN_7>,
    /// See [`Button::VolumeDown`].
    pub switch_volume_down: Peri<'d, PIN_8>,
    /// See [`Button::BrightnessUp`].
    pub switch_brightness_up: Peri<'d, PIN_21>,
    /// See [`Button::BrightnessDown`].
    pub switch_brightness_down: Peri<'d, PIN_26>,
}

/// Fill [`Peripherals`] from `embassy_rp::init`'s result and a bound `Pio`.
///
/// ```ignore
/// let p = embassy_rp::init(Default::default());
/// let pio = Pio::new(p.PIO0, Irqs);
/// let mut unicorn = GalacticUnicorn::new(galactic_unicorn::peripherals!(p, pio));
/// ```
#[macro_export]
macro_rules! peripherals {
    ($p:ident, $pio:ident) => {
        $crate::Peripherals {
            pio: $pio,
            dma_data: $p.DMA_CH0,
            dma_ctrl: $p.DMA_CH1,
            adc: $p.ADC,
            column_clock: $p.PIN_13,
            column_data: $p.PIN_14,
            column_latch: $p.PIN_15,
            column_blank: $p.PIN_16,
            row_bit_0: $p.PIN_17,
            row_bit_1: $p.PIN_18,
            row_bit_2: $p.PIN_19,
            row_bit_3: $p.PIN_20,
            light_sensor: $p.PIN_28,
            switch_a: $p.PIN_0,
            switch_b: $p.PIN_1,
            switch_c: $p.PIN_3,
            switch_d: $p.PIN_6,
            switch_sleep: $p.PIN_27,
            switch_volume_up: $p.PIN_7,
            switch_volume_down: $p.PIN_8,
            switch_brightness_up: $p.PIN_21,
            switch_brightness_down: $p.PIN_26,
        }
    };
}

/// The display driver (`pimoroni::GalacticUnicorn`).
///
/// Create it once with [`GalacticUnicorn::new`] (C++ `init()`).
pub struct GalacticUnicorn<'d> {
    bitstream: &'static mut Bitstream,
    /// `0..=256`, applied as `(channel * brightness) >> 8`.
    brightness: u16,
    adc: Adc<'d, embassy_rp::adc::Blocking>,
    light: AdcChannel<'d>,
    buttons: [Input<'d>; 9],
    // Keep the PIO program and state machine alive while DMA feeds them.
    _common: Common<'d, PIO0>,
    _sm: StateMachine<'d, PIO0, 0>,
}

impl<'d> GalacticUnicorn<'d> {
    /// Display width in pixels (53).
    pub const WIDTH: usize = WIDTH;
    /// Display height in pixels (11).
    pub const HEIGHT: usize = HEIGHT;

    /// Configure the driver chips, start the PIO and the looping DMA.
    ///
    /// Panics if called twice (the bitstream is a singleton).
    pub fn new(mut p: Peripherals<'d>) -> Self {
        let bitstream = BITSTREAM.init(Bitstream::new());
        BITSTREAM_ADDR.store(bitstream.0.as_ptr() as u32, Ordering::Relaxed);

        // Bit-bang the driver chip configuration with the display blanked and
        // an invisible row selected.
        {
            let mut clock = Output::new(p.column_clock.reborrow(), Level::Low);
            let mut data = Output::new(p.column_data.reborrow(), Level::Low);
            let mut latch = Output::new(p.column_latch.reborrow(), Level::Low);
            let mut blank = Output::new(p.column_blank.reborrow(), Level::High);
            let _rows = [
                Output::new(p.row_bit_0.reborrow(), Level::High),
                Output::new(p.row_bit_1.reborrow(), Level::High),
                Output::new(p.row_bit_2.reborrow(), Level::High),
                Output::new(p.row_bit_3.reborrow(), Level::High),
            ];
            block_for(Duration::from_millis(100));

            let mut write_bit = |bit: bool, latch: Option<&mut Output>| {
                data.set_level(bit.into());
                block_for(Duration::from_micros(10));
                clock.set_high();
                block_for(Duration::from_micros(10));
                clock.set_low();
                if let Some(latch) = latch {
                    latch.set_high();
                }
            };
            // The first 9 chips get the value, the last one is clocked and latched.
            for _ in 0..9 {
                for i in 0..16 {
                    write_bit(REG1 & (1 << (15 - i)) != 0, None);
                }
            }
            for i in 0..16 {
                write_bit(REG1 & (1 << (15 - i)) != 0, (i == 4).then_some(&mut latch));
            }
            latch.set_low();

            // Reapply blank: the above leaves a slight glow.
            blank.set_low();
            block_for(Duration::from_micros(10));
            blank.set_high();
        }

        let Pio {
            mut common, mut sm0, ..
        } = p.pio;

        let prg = pio::pio_asm!(
            ".side_set 1 opt",
            ".wrap_target",
            // loop over row pixels
            "  out y, 8",    // row pixel count (minus 1: test is pre-decrement)
            "  out pins, 8", // row select
            "pixels:",
            // 1st bit in the stream (blue)
            "  out x, 1       side 0 [1]",
            "  set pins, 0b100",
            "  jmp !x endb",
            "  set pins, 0b101",
            "endb:",
            "  nop            side 1 [2]",
            // green
            "  out x, 1       side 0 [1]",
            "  set pins, 0b100",
            "  jmp !x endg",
            "  set pins, 0b101",
            "endg:",
            "  nop            side 1 [2]",
            // red, then discard the five dummy bits of this pixel
            "  out x, 1       side 0 [1]",
            "  set pins, 0b100",
            "  jmp !x endr",
            "  set pins, 0b101",
            "endr:",
            "  out null, 5    side 1 [2]",
            "  jmp y--, pixels",
            "  out null, 8", // discard dword-alignment byte
            "  set pins, 0b110 [5]", // latch high, blank high
            "  set pins, 0b000", // blank low: output enabled
            // loop over the bcd delay period
            "  out y, 32",
            "bcd_delay:",
            "  jmp y--, bcd_delay",
            "  set pins, 0b100", // blank high: output disabled
            ".wrap",
        );
        let loaded = common.load_program(&prg.program);

        let clock = common.make_pio_pin(p.column_clock);
        let data = common.make_pio_pin(p.column_data);
        let latch = common.make_pio_pin(p.column_latch);
        let blank = common.make_pio_pin(p.column_blank);
        let rows = [
            common.make_pio_pin(p.row_bit_0),
            common.make_pio_pin(p.row_bit_1),
            common.make_pio_pin(p.row_bit_2),
            common.make_pio_pin(p.row_bit_3),
        ];
        let [r0, r1, r2, r3] = &rows;

        // Blank and rows high first, then make everything an output, to avoid a flash.
        sm0.set_pins(Level::High, &[&blank, r0, r1, r2, r3]);
        sm0.set_pin_dirs(Direction::Out, &[&clock, &data, &latch, &blank, r0, r1, r2, r3]);

        let mut cfg = Config::default();
        cfg.use_program(&loaded, &[&clock]);
        cfg.set_out_pins(&[r0, r1, r2, r3]);
        cfg.set_set_pins(&[&data, &latch, &blank]);
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 32,
            direction: ShiftDirection::Right,
        };
        cfg.fifo_join = FifoJoin::TxOnly; // 8 deep instead of 4
        sm0.set_config(&cfg);

        start_dma::<DMA_CH0, DMA_CH1>();
        sm0.set_enable(true);
        // Kick off the control channel; it triggers the data channel via chaining.
        pac::DMA
            .multi_chan_trigger()
            .write_value(pac::dma::regs::MultiChanTrigger(1 << DMA_CH1::number()));

        let adc = Adc::new_blocking(p.adc, AdcConfig::default());
        let light = AdcChannel::new_pin(p.light_sensor, Pull::None);
        let up = Pull::Up;
        let buttons = [
            Input::new(p.switch_a, up),
            Input::new(p.switch_b, up),
            Input::new(p.switch_c, up),
            Input::new(p.switch_d, up),
            Input::new(p.switch_sleep, up),
            Input::new(p.switch_volume_up, up),
            Input::new(p.switch_volume_down, up),
            Input::new(p.switch_brightness_up, up),
            Input::new(p.switch_brightness_down, up),
        ];

        Self {
            bitstream,
            brightness: 256,
            adc,
            light,
            buttons,
            _common: common,
            _sm: sm0,
        }
    }

    /// Turn every pixel off on the next frame (`clear`). Not needed if you
    /// [`update`](Self::update) the whole display anyway.
    pub fn clear(&mut self) {
        for y in 0..HEIGHT as i32 {
            for x in 0..WIDTH as i32 {
                self.set_pixel(x, y, 0, 0, 0);
            }
        }
    }

    /// Set one pixel, `x` in `0..53`, `y` in `0..11`; out-of-range is ignored (`set_pixel`).
    ///
    /// The colour is brightness-scaled and gamma-corrected. It shows from the next
    /// frame, so when drawing a full image keep the time between calls short or you
    /// will see scanning artefacts. Prefer [`update`](Self::update).
    pub fn set_pixel(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8) {
        self.bitstream.set_pixel(x, y, r, g, b, self.brightness);
    }

    /// Copy a framebuffer to the display with gamma correction (`update`).
    ///
    /// The recommended way to update the image. You can keep several framebuffers
    /// and switch between them by passing a different one.
    pub fn update(&mut self, graphics: &PenRgb888<WIDTH, HEIGHT>) {
        for (y, row) in graphics.frame_buffer().iter().enumerate() {
            for (x, &col) in row.iter().enumerate() {
                self.set_pixel(x as i32, y as i32, (col >> 16) as u8, (col >> 8) as u8, col as u8);
            }
        }
    }

    /// Set brightness, clamped to `0.0..=1.0` (`set_brightness`). Applies to pixels
    /// written after this call, so redraw to see it.
    pub fn set_brightness(&mut self, value: f32) {
        self.brightness = (value.clamp(0.0, 1.0) * 256.0) as u16;
    }

    /// Current brightness, `0.0..=1.0` (`get_brightness`).
    pub fn brightness(&self) -> f32 {
        self.brightness as f32 / 256.0
    }

    /// Add `delta` to the brightness, then clamp to `0.0..=1.0` (`adjust_brightness`).
    pub fn adjust_brightness(&mut self, delta: f32) {
        self.set_brightness(self.brightness() + delta);
    }

    /// Ambient light sensor reading, `0..=4095` (`light`). Returns 0 if the ADC read fails.
    pub fn light(&mut self) -> u16 {
        self.adc.blocking_read(&mut self.light).unwrap_or(0)
    }

    /// True while `button` is held (`is_pressed`).
    pub fn is_pressed(&self, button: Button) -> bool {
        self.buttons[button as usize].is_low()
    }
}

/// Chain `Data` and `Ctrl` into an endless loop feeding PIO0 SM0.
fn start_dma<Data: ChannelInstance, Ctrl: ChannelInstance>() {
    let data = Data::regs();
    let ctrl = Ctrl::regs();

    // Control channel: copy BITSTREAM_ADDR into the data channel's read address
    // (non-triggering alias), then chain into the data channel.
    ctrl.read_addr().write_value(BITSTREAM_ADDR.as_ptr() as u32);
    ctrl.write_addr().write_value(data.read_addr().as_ptr() as u32);
    ctrl.trans_count().write(|w| {
        w.set_mode(0.into());
        w.set_count(1);
    });
    let mut c = CtrlTrig(0);
    c.set_treq_sel(TreqSel::PERMANENT);
    c.set_data_size(DataSize::SIZE_WORD);
    c.set_incr_read(false);
    c.set_incr_write(false);
    c.set_chain_to(Data::number());
    c.set_en(true);
    ctrl.al1_ctrl().write_value(c.0);

    // Data channel: stream the bitstream into the PIO TX FIFO, then chain back.
    data.write_addr().write_value(pac::PIO0.txf(0).as_ptr() as u32);
    data.trans_count().write(|w| {
        w.set_mode(0.into());
        w.set_count((BITSTREAM_LENGTH / 4) as u32);
    });
    let mut c = CtrlTrig(0);
    c.set_treq_sel(TreqSel::from(0)); // PIO0 TX0
    c.set_data_size(DataSize::SIZE_WORD);
    c.set_incr_read(true);
    c.set_incr_write(false);
    c.set_chain_to(Ctrl::number());
    c.set_en(true);
    data.al1_ctrl().write_value(c.0);

    compiler_fence(Ordering::SeqCst);
}
