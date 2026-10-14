# Galactic Unicorn driver (Rust)

Rust port of Pimoroni's `galactic_unicorn` for the RP2350 (Pico 2 W). Crate: `crates/galactic-unicorn`.
Mirrors the C++ [README](../ref-impl/libraries/galactic_unicorn/README.md) topic by topic.

- **Display:** 53×11 RGB LEDs, driven by the PIO
- **Gamma + precision:** gamma-corrected, refreshed with 14-bit precision (binary-coded modulation)
- **Refresh:** the C++ README claims about 300 fps on RP2040; the port runs the same PIO program and has not been measured
- **Drawing:** via [`pico-graphics`](pico-graphics.md), then `update()`
- **Audio:** an I2S output fed by DMA, with samples or the [synth](pico-synth.md) (see [Audio](#audio))

## How it fits together

```mermaid
graph LR
    App[App code] --> Pen[pico-graphics PenRgb888]
    EG[embedded-graphics] --> Pen
    Pen -- update --> GU[GalacticUnicorn]
    GU -- set_pixel: gamma + brightness --> BS[Bitstream 9,240 B]
    BS -- looping DMA --> PIO[PIO state machine 0]
    PIO --> LED[Column chips + row select]
    Btn[Buttons, light sensor] --> GU
    Synth[pico-synth] --> GU
    Smp[Sample buffer] --> GU
    GU -- DMA + IRQ --> I2S[PIO state machine 1: I2S]
    I2S --> Amp[Amplifier + speaker]
```

Once started, the CPU is not involved in refreshing the display. It only writes the bitstream.

## Getting started

```rust
bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => galactic_unicorn::AudioInterruptHandler;
});

let p = embassy_rp::init(Default::default());
let pio = Pio::new(p.PIO0, Irqs);
let mut unicorn = GalacticUnicorn::new(galactic_unicorn::peripherals!(p, pio), Irqs);
unicorn.set_brightness(0.5);

let mut graphics = PenRgb888::<WIDTH, HEIGHT>::new();
graphics.set_pen_rgb(255, 255, 0);
graphics.set_pixel(Point::new(10, 5));
unicorn.update(&graphics);
```

- A complete program is `src/main.rs` (rainbow plus a sweeping dot, logging over USB serial)
- `GalacticUnicorn::new` replaces C++ `init()`. It is a **singleton**: a second call panics
- `peripherals!` moves the pins, DMA channels, ADC and PIO into the driver
- The second argument proves you bound `AudioInterruptHandler` to `DMA_IRQ_0` (see [Audio](#audio))
- Flash with `./scripts/run.sh` (see [Flashing](../README.md#flashing))

## Interleaved framebuffer

The PIO has no random memory access and almost no branching. It can only process a stream in order. So the framebuffer is **interleaved**: each BCD frame is delivered one after another, with the row, pixel count and timing values inserted where the PIO program needs them.

```mermaid
graph TD
    Buf["Bitstream: 9,240 B"] --> Row["11 rows × 840 B"]
    Row --> Frame["14 BCD frames × 60 B"]
    Frame --> B0["byte 0: pixel count − 1 = 52"]
    Frame --> B1["byte 1: row select"]
    Frame --> B2["bytes 2–54: 53 pixels, xxxxxrgb"]
    Frame --> B3["byte 55: padding (dword align)"]
    Frame --> B4["bytes 56–59: BCD tick count = 1 << frame"]
```

| Bytes | Content                                         |
| ----- | ----------------------------------------------- |
| 0     | pixel count minus one (52)                      |
| 1     | row select (0–10)                               |
| 2–54  | one byte per pixel: bit 0 blue, 1 green, 2 red  |
| 55    | dummy byte                                      |
| 56–59 | BCD delay ticks, little endian, `1 << frame`    |

- **Gamma:** each channel goes through a 256-entry table to 14 bits. Bit *n* of the result is written into BCD frame *n*
- **Brightness:** applied before gamma as `(channel × brightness) >> 8`, with brightness in `0..=256`
- **Flip:** coordinates are mirrored (`x → 52 − x`, `y → 10 − y`) to match how the panel is wired
- **Note:** the C++ README's byte table says pixels are at 1–53 and row select at 56. The C++ *code* (and this port) uses the layout above

You do not need to worry about any of this when using the driver. The layout lives in `bitstream.rs`, which is pure Rust and unit-tested on the host.

## Function reference

### System state

| C++                        | Rust                                  | Status |
| -------------------------- | ------------------------------------- | ------ |
| `init()`                   | `GalacticUnicorn::new(peripherals)`   | ✅     |
| `set_brightness(f32)`      | `set_brightness(f32)`                 | ✅     |
| `get_brightness()`         | `brightness()`                        | ✅     |
| `adjust_brightness(f32)`   | `adjust_brightness(f32)`              | ✅     |
| `set_volume(f32)`          | `set_volume(f32)`                     | ✅     |
| `get_volume()`             | `volume()`                            | ✅     |
| `adjust_volume(f32)`       | `adjust_volume(f32)`                  | ✅     |
| `light()`                  | `light()`                             | ✅     |
| `is_pressed(button)`       | `is_pressed(Button)`                  | ✅     |

- **Brightness:** clamped to `0.0..=1.0`. It applies to pixels written *after* the call, so redraw to see the change
- **`light()`:** `0..=4095`, from the ADC on GPIO 28 (blocking read). Returns `0` if the read fails

```rust
unicorn.set_brightness(0.5);
unicorn.adjust_brightness(0.1);  // 0.6
unicorn.adjust_brightness(0.7);  // 1.0
unicorn.adjust_brightness(-0.2); // 0.8
```

### Buttons

C++ passes a GPIO number; Rust uses the `Button` enum.

| `Button`         | GPIO | C++ constant            |
| ---------------- | ---- | ----------------------- |
| `A`              | 0    | `SWITCH_A`              |
| `B`              | 1    | `SWITCH_B`              |
| `C`              | 3    | `SWITCH_C`              |
| `D`              | 6    | `SWITCH_D`              |
| `Sleep`          | 27   | `SWITCH_SLEEP`          |
| `VolumeUp`       | 7    | `SWITCH_VOLUME_UP`      |
| `VolumeDown`     | 8    | `SWITCH_VOLUME_DOWN`    |
| `BrightnessUp`   | 21   | `SWITCH_BRIGHTNESS_UP`  |
| `BrightnessDown` | 26   | `SWITCH_BRIGHTNESS_DOWN`|

- Active low with pull-ups; `is_pressed` is true while held
- Brightness, sleep and volume buttons are **not** tied to hardware functions. They are software-only, so you can use them for anything

```rust
while !unicorn.is_pressed(Button::A) {}
info!("We did it! We pressed switch A! Heck yeah!");
```

### Drawing

| C++                             | Rust                          | Status |
| ------------------------------- | ----------------------------- | ------ |
| `update(PicoGraphics*)`         | `update(&PenRgb888<53, 11>)`  | ⚠️ RGB888 only |
| `clear()`                       | `clear()`                     | ✅     |
| `set_pixel(x, y, r, g, b)`      | `set_pixel(x, y, r, g, b)`    | ✅     |

- **`update`:** the recommended way to draw. It copies the framebuffer into the bitstream with gamma applied. Keep several framebuffers and switch by passing a different one. The C++ version also accepts RGB565, RGB332, P8 and P4 buffers; the port has only RGB888 (see [pico-graphics](pico-graphics.md))
- **`set_pixel`:** `x` in `0..53`, `y` in `0..11`; out of range is ignored. Colour is `0..=255` per channel, gamma corrected automatically. It shows from the next frame, so when drawing a full image keep the time between calls short to avoid scanning artefacts
- **`clear`:** turns the display off from the next frame. Unnecessary if you `update` the whole display anyway

### Audio

Audio goes out as I2S through a second PIO state machine to the on-board amplifier. It is fed by one DMA channel (`DMA_CH2`) from either a sample buffer or the [synth](pico-synth.md).

```mermaid
graph TD
    Smp["play_sample(&'static [i16])"] --> DMA[audio DMA channel, 16-bit]
    Syn[pico-synth: 8 voices] --> Buf["2 × 4-sample buffers"]
    Buf --> DMA
    DMA -- "DREQ: PIO0 TX1" --> SM1[PIO state machine 1: I2S]
    SM1 -- "data 9, BCLK 10, LRCLK 11" --> Amp[Amplifier]
    DMA -- "done: DMA_IRQ_0" --> IRQ[AudioInterruptHandler]
    IRQ -- "start next buffer, refill the other" --> Buf
```

| C++                                   | Rust                                          | Status |
| ------------------------------------- | --------------------------------------------- | ------ |
| `play_sample(uint8_t*, length)`       | `play_sample(&'static [i16])`                 | ⚠️ takes samples |
| `synth_channel(uint)` → reference     | `synth_channel(usize, \|c\| ...)`             | ⚠️ closure |
| (direct `synth` access)               | `with_synth(\|synth\| ...)`                   | ➕ added |
| `play_synth()`                        | `play_synth()`                                | ✅     |
| `stop_playing()`                      | `stop_playing()`                              | ✅     |
| `set/get/adjust_volume`               | `set_volume`, `volume`, `adjust_volume`       | ✅     |

```rust
unicorn.synth_channel(0, |c| {
    c.waveforms = Waveform::SINE;
    c.frequency = 440;
    c.attack_ms = 10;
    c.trigger_attack();
});
unicorn.play_synth();
// later: unicorn.synth_channel(0, |c| c.trigger_release());
```

- **Samples:** mono, signed 16-bit, **22,050 Hz**. They are played from your buffer by DMA, so the slice must be `'static`. The C++ README says `length` is a number of samples, but the C++ code treats it as bytes (`length / 2` transfers). The port takes a slice of samples, which avoids the confusion
- **Synth:** `play_synth` starts the stream; keep notes going by triggering envelopes on the channels. Playing stops when `stop_playing` is called or another source replaces it. The sound is generated 4 samples at a time inside the DMA interrupt
- **Shared synth:** the interrupt and your code both use the synth, so it sits behind a critical section. `synth_channel` and `with_synth` take a closure that runs inside it: keep it short
- **Volume:** `0.0..=1.0`. It also sets the synth master volume (`volume × 255`). As in C++, the driver starts at 127 (about 0.5), but the synth master starts at `0x2fff` until you call `set_volume`
- **MUTE:** GPIO 22 is driven high at start, as the C++ does

#### Binding the interrupt

The audio DMA interrupt is `DMA_IRQ_0`. Bind `AudioInterruptHandler` to it and pass the binding to `new`:

```rust
bind_interrupts!(struct Irqs {
    DMA_IRQ_0 => galactic_unicorn::AudioInterruptHandler;
});
```

- `DMA_IRQ_0` is shared: if you also use embassy's own DMA driver, list its handler in the same `bind_interrupts!` entry
- The driver owns `DMA_CH0` (bitstream data), `DMA_CH1` (bitstream control) and `DMA_CH2` (audio)

### Constants

| C++                           | Rust                                        |
| ----------------------------- | ------------------------------------------- |
| `GalacticUnicorn::WIDTH` (53) | `galactic_unicorn::WIDTH`, `GalacticUnicorn::WIDTH`  |
| `GalacticUnicorn::HEIGHT` (11)| `galactic_unicorn::HEIGHT`, `GalacticUnicorn::HEIGHT` |

```rust
let num_pixels = WIDTH * HEIGHT;
```

Pin numbers are fixed by the hardware and listed in [driver-port.md](driver-port.md#pins).

## Differences from the C++ driver

| Topic         | C++                                  | Rust                                      |
| ------------- | ------------------------------------ | ----------------------------------------- |
| Setup         | `init()` can be called again         | `new()` once; second call panics          |
| Teardown      | destructor aborts DMA, frees PIO     | none; the driver lives for the program    |
| Audio IRQ     | installed by the driver              | you bind `AudioInterruptHandler`          |
| Shared synth  | plain member                         | behind a critical section (IRQ uses it)   |
| Peripherals   | claimed by number at runtime         | moved in through `Peripherals`            |
| Buffer writes | plain stores                         | volatile stores (DMA reads concurrently)  |
| Buttons       | GPIO number                          | `Button` enum                             |
| Delays        | `sleep_ms` / `sleep_us`              | `embassy_time::block_for`                 |
| Chip          | RP2040                               | RP2350 (same PIO program)                 |

## Tests

- `bitstream.rs` is tested on the host: header and ticks, channel-to-bit mapping, brightness, bounds
  - `cargo test -p galactic-unicorn --target aarch64-apple-darwin`
- The synth is tested in [pico-synth](pico-synth.md)
- The PIO and DMA code (display and audio) can only be checked on the board
