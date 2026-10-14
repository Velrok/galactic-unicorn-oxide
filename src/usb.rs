//! USB device: a CDC-ACM serial port for logs plus picotool's reset interface.
//!
//! The reset interface lets `picotool reboot -f -u` (and so `cargo run`) drop a
//! running board into BOOTSEL without touching the button.

use embassy_futures::join::join;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::control::{InResponse, OutResponse, Recipient, Request, RequestType};
use embassy_usb::types::InterfaceNumber;
use embassy_usb::{Builder, Config, Handler};
use static_cell::StaticCell;

/// Class/subclass/protocol picotool looks for.
const RESET_CLASS: u8 = 0xFF;
const RESET_SUBCLASS: u8 = 0x00;
const RESET_PROTOCOL: u8 = 0x01;

/// pico-sdk `reset_interface` requests.
const RESET_REQUEST_BOOTSEL: u8 = 0x01;
const RESET_REQUEST_FLASH: u8 = 0x02;

struct ResetHandler {
    interface: InterfaceNumber,
}

impl Handler for ResetHandler {
    fn control_out(&mut self, req: Request, _data: &[u8]) -> Option<OutResponse> {
        if req.request_type != RequestType::Class
            || req.recipient != Recipient::Interface
            || req.index != u8::from(self.interface) as u16
        {
            return None;
        }
        match req.request {
            RESET_REQUEST_BOOTSEL => {
                embassy_rp::rom_data::reset_to_usb_boot(0, 0);
                Some(OutResponse::Accepted)
            }
            RESET_REQUEST_FLASH => cortex_m::peripheral::SCB::sys_reset(),
            _ => Some(OutResponse::Rejected),
        }
    }

    fn control_in<'a>(&'a mut self, _req: Request, _buf: &'a mut [u8]) -> Option<InResponse<'a>> {
        None
    }
}

/// Run the USB device and the serial logger. Never returns.
pub async fn run(driver: Driver<'static, USB>) -> ! {
    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<State> = StaticCell::new();
    static RESET: StaticCell<ResetHandler> = StaticCell::new();

    // Raspberry Pi's VID with the pico-sdk CDC PID, which picotool recognises.
    let mut config = Config::new(0x2e8a, 0x000a);
    config.manufacturer = Some("galactic-unicorn-oxide");
    config.product = Some("Galactic Unicorn");
    // picotool matches the running app to the BOOTSEL device by serial number,
    // which the boot ROM sets to the chip ID.
    static SERIAL: StaticCell<[u8; 16]> = StaticCell::new();
    if let Ok(id) = embassy_rp::otp::get_chipid() {
        let hex = SERIAL.init([0; 16]);
        for (i, c) in hex.iter_mut().enumerate() {
            *c = b"0123456789ABCDEF"[(id >> (60 - 4 * i) & 0xf) as usize];
        }
        config.serial_number = core::str::from_utf8(hex).ok();
    }
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; 64]),
    );

    let class = CdcAcmClass::new(&mut builder, CDC_STATE.init(State::new()), 64);

    let mut function = builder.function(RESET_CLASS, RESET_SUBCLASS, RESET_PROTOCOL);
    let mut interface = function.interface();
    let interface_number = interface.interface_number();
    interface.alt_setting(RESET_CLASS, RESET_SUBCLASS, RESET_PROTOCOL, None);
    drop(function);
    builder.handler(RESET.init(ResetHandler {
        interface: interface_number,
    }));

    let mut device = builder.build();
    let log = embassy_usb_logger::with_class!(1024, log::LevelFilter::Info, class);
    join(device.run(), log).await;
    unreachable!()
}
