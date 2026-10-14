# Galactic Unicorn driver: port to Rust

- **Board:** RP2350 (Pico 2 W)
- **Decision:** port the driver, don't use FFI
- **Source:** [pimoroni-pico](https://github.com/pimoroni/pimoroni-pico) → `libraries/galactic_unicorn` (b41010d)

## Why not FFI

- The "C SDK" is C++ (`pimoroni::GalacticUnicorn`) on pico-sdk
- pico-sdk and embassy-rp both want startup, clocks, interrupts, `pio0`, DMA and `DMA_IRQ_0`
- Build would need CMake + Arm GCC + Cargo

## Target stack

```mermaid
graph TD
    App[App code] --> EG[embedded-graphics DrawTarget]
    EG --> Drv[Driver: set_pixel, buttons, light]
    Drv --> Buf[Static bitstream buffer]
    Drv --> Emb[embassy-rp]
    Emb --> PIO[PIO program via pio crate]
    Emb --> DMA[Looping DMA]
    Buf --> DMA --> PIO --> LED[Column drivers + row select]
```

## Display pipeline

- `set_pixel()` only writes the buffer (brightness + gamma applied first)
- Two chained DMA channels loop the buffer into the PIO forever
- PIO shifts pixels out and selects rows with no CPU involved

```mermaid
graph LR
    CPU -- set_pixel --> Buffer[9,240 B buffer]
    Buffer -- DMA loop --> PIO
    PIO -- CLOCK/DATA/LATCH/BLANK --> Cols[10 column chips]
    PIO -- row select --> Rows[Pins 17-20]
```

## Bitstream layout

- 11 rows × 14 BCD sub-frames × 60 bytes = **9,240 bytes**

```mermaid
graph TD
    Buf[Buffer 9,240 B] --> Row[11 rows]
    Row --> Sub[14 BCD sub-frames per row]
    Sub --> Frame[60-byte frame]
    Frame --> B0["byte 0: pixel count - 1"]
    Frame --> B1["byte 1: row select"]
    Frame --> B2["bytes 2-54: pixels, xxxxxbgr"]
    Frame --> B3["bytes 55: padding"]
    Frame --> B4["bytes 56-59: BCD tick = 1 << frame"]
```

## Pins

| Group       | Pins                                   |
| ----------- | -------------------------------------- |
| Column      | CLOCK 13, DATA 14, LATCH 15, BLANK 16  |
| Row select  | 17, 18, 19, 20                         |
| Light (ADC) | 28                                     |
| Switches    | A 0, B 1, C 3, D 6, SLEEP 27           |
| Vol / Bri   | VOL+ 7, VOL− 8, BRIGHT+ 21, BRIGHT− 26 |
| Audio (skip)| MUTE 22, I2S 9/10/11, I2C 4/5          |

## Port order

1. Fill the buffer with row selects and BCD timing values
2. Bit-bang driver-chip config `0b1111111111001110` (full current)
3. Start PIO + looping DMA
4. `set_pixel`, then `is_pressed` and `light()`
5. Audio: later

## References

- C++ source and `.pio` file in the repo above
- [Schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/galactic_unicorn_schematic.pdf?v=1667910096)
- [embassy-rp](https://docs.embassy.dev/embassy-rp/) · [pio crate](https://docs.rs/crate/pio/latest) · [embedded-graphics](https://docs.rs/embedded-graphics)
