# galactic-unicorn-oxide

Bare-metal Rust firmware and drivers for the Pimoroni Galactic Unicorn (RP2350, Pico 2 W).
Ported from [pimoroni-pico](https://github.com/pimoroni/pimoroni-pico) (MIT), kept as a submodule in `ref-impl/`.

| Crate                      | What it is                                         |
| -------------------------- | -------------------------------------------------- |
| `crates/galactic-unicorn`  | display driver (PIO + looping DMA), buttons, light |
| `crates/pico-graphics`     | RGB888 framebuffer, `Rect`, `Point`, `Rgb`         |
| `src/`                     | demo firmware: rainbow + logging over USB serial   |

## Docs

- [Galactic Unicorn driver](docs/galactic-unicorn.md): API, framebuffer layout, what is ported
- [Pico Graphics](docs/pico-graphics.md): API and C++ mapping
- [Driver port plan](docs/driver-port.md): decisions, pipeline, pins

## Flashing

No debug probe needed.

```sh
git submodule update --init   # reference C++ implementation (optional)
./scripts/run.sh              # build, flash over USB, attach to the serial log
```

- `cargo run --release` flashes by itself: the firmware has a picotool reset interface, so the running board reboots into BOOTSEL on its own
- **First flash only:** hold BOOTSEL while plugging in, because the old firmware has no reset interface
- Logs come over USB serial: `screen /dev/cu.usbmodem* 115200` (quit with `Ctrl-A K`)

## Tests

```sh
cargo test -p pico-graphics -p galactic-unicorn --target aarch64-apple-darwin
```

The driver itself (PIO, DMA) only builds for `thumbv8m.main-none-eabihf`, so the host tests cover the pure logic.

## Licence

The port derives from MIT-licensed code by Pimoroni Ltd; see `ref-impl/LICENSE`.
