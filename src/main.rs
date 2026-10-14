#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIO0, USB};
use embassy_rp::pio::{self, Pio};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use galactic_unicorn::{GalacticUnicorn, HEIGHT, WIDTH};
use log::info;
use panic_halt as _;
use pico_graphics::{PenRgb888, Point};

mod usb;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
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
    let mut unicorn = GalacticUnicorn::new(galactic_unicorn::peripherals!(p, pio));
    unicorn.set_brightness(0.1);
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
        unicorn.update(&graphics);

        if frame.is_multiple_of(60) {
            info!("frame {frame}, light {}", unicorn.light());
        }
        frame = frame.wrapping_add(1);
        Timer::after(Duration::from_millis(33)).await;
    }
}
