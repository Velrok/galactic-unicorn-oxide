#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIO0, USB};
use embassy_rp::pio::{self, Pio};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use galactic_unicorn::pico_synth::{SAMPLE_RATE, SINE_WAVEFORM, Waveform};
use galactic_unicorn::{AudioInterruptHandler, Button, GalacticUnicorn, HEIGHT, WIDTH};
use log::info;
use panic_halt as _;
use pico_graphics::{PenRgb888, Point};

mod usb;

/// 100 ms of an 880 Hz sine, to try `play_sample`.
static BEEP: [i16; 2205] = beep();

const fn beep() -> [i16; 2205] {
    let mut samples = [0i16; 2205];
    let mut i = 0;
    while i < samples.len() {
        // Table index: 256 steps per period.
        let phase = (i as u32 * 880 * 256 / SAMPLE_RATE) % 256;
        samples[i] = SINE_WAVEFORM[phase as usize] / 4;
        i += 1;
    }
    samples
}

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => AudioInterruptHandler;
});

#[embassy_executor::task]
async fn usb_task(driver: Driver<'static, USB>) {
    usb::run(driver).await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    spawner.spawn(usb_task(Driver::new(p.USB, Irqs)).unwrap());

    let pio = Pio::new(p.PIO0, Irqs);
    let mut unicorn = GalacticUnicorn::new(galactic_unicorn::peripherals!(p, pio), Irqs);
    unicorn.set_brightness(0.5);
    unicorn.set_volume(0.5);
    // Note voices: A = sine 440 Hz, B = square 660 Hz (held while pressed).
    unicorn.synth_channel(0, |c| {
        c.waveforms = Waveform::SINE;
        c.frequency = 440;
        c.attack_ms = 10;
        c.decay_ms = 100;
        c.sustain = 0xafff;
        c.release_ms = 100;
    });
    unicorn.synth_channel(1, |c| {
        c.waveforms = Waveform::SQUARE;
        c.frequency = 660;
        c.attack_ms = 10;
        c.decay_ms = 100;
        c.sustain = 0x7fff;
        c.release_ms = 100;
    });
    let mut held = [false; 2];
    let mut c_held = false;
    info!("galactic unicorn up, {WIDTH}x{HEIGHT}");

    let mut graphics = PenRgb888::<WIDTH, HEIGHT>::new();
    let mut frame: u32 = 0;
    loop {
        // Rainbow background with a white dot sweeping along the middle row.
        for x in 0..WIDTH as i32 {
            let hue = ((x as u32 * 4 + frame) % 256) as f32 / 256.0;
            let pen = graphics.create_pen_hsv(hue, 1.0, 0.4);
            graphics.set_pen(pen);
            for y in 0..HEIGHT as i32 {
                graphics.set_pixel(Point::new(x, y));
            }
        }
        graphics.set_pen_rgb(255, 255, 255);
        graphics.set_pixel(Point::new((frame % WIDTH as u32) as i32, HEIGHT as i32 / 2));
        // A and B play notes while held, C plays a beep sample, D stops everything.
        for (i, button) in [Button::A, Button::B].into_iter().enumerate() {
            let pressed = unicorn.is_pressed(button);
            if pressed && !held[i] {
                unicorn.synth_channel(i, |c| c.trigger_attack());
                unicorn.play_synth();
            } else if !pressed && held[i] {
                unicorn.synth_channel(i, |c| c.trigger_release());
            }
            held[i] = pressed;
        }
        let c_pressed = unicorn.is_pressed(Button::C);
        if c_pressed && !c_held {
            unicorn.play_sample(&BEEP);
        }
        c_held = c_pressed;
        if unicorn.is_pressed(Button::D) {
            unicorn.stop_playing();
        }
        // Volume buttons ramp the volume like the brightness ones.
        if unicorn.is_pressed(Button::VolumeUp) {
            unicorn.adjust_volume(0.01);
        }
        if unicorn.is_pressed(Button::VolumeDown) {
            unicorn.adjust_volume(-0.01);
        }
        // Hold the brightness buttons to ramp the brightness (~0.3 per second).
        if unicorn.is_pressed(Button::BrightnessUp) {
            unicorn.adjust_brightness(0.01);
        }
        if unicorn.is_pressed(Button::BrightnessDown) {
            unicorn.adjust_brightness(-0.01);
        }
        unicorn.update(&graphics);

        if frame.is_multiple_of(60) {
            info!("frame {frame}, light {}, brightness {:.2}, volume {:.2}", unicorn.light(), unicorn.brightness(), unicorn.volume());
        }
        frame = frame.wrapping_add(1);
        Timer::after(Duration::from_millis(33)).await;
    }
}
