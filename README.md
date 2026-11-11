# ESP32-S31-Korvo-1 Weather & Outfit Advisor (Rust, no_std)

A `no_std` Rust demo for the **ESP32-S31-Korvo-1** board: fetches current
weather from [Open-Meteo](https://open-meteo.com) over Wi-Fi and shows
temperature, conditions, outfit advice, rain/snow alerts and UV warnings as
short phrases on the 800x480 RGB565 LCD. Refreshes every 30 minutes, retries
on Wi-Fi/API failure, runs forever.

LCD driver and pin map are taken **verbatim** from
[Hahihula/s31-korvo-1-rust-examples](https://github.com/Hahihula/s31-korvo-1-rust-examples)
(`esp32-s31-korvo/src/display.rs`) — the user reproduces that exact hardware
setup, so no wiring changes are needed relative to the reference demos.
Screen styling follows
[georgik/esp32-conways-game-of-life-rs](https://github.com/georgik/esp32-conways-game-of-life-rs).

## Build & flash

The ESP32-S31 is a RISC-V chip, so plain **stable Rust** works — no espup
needed, only the target and `rust-src` (pinned in `rust-toolchain.toml`):

```bash
rustup target add riscv32imafc-unknown-none-elf
rustup component add rust-src
cargo install cargo-espflash --locked
```

Wi-Fi credentials, and optionally the city, are compile-time environment
variables (default city: `Brno`, Czechia). `build.rs` forwards them to the
compiler, so they also work when the build is launched from an IDE; cargo
recompiles automatically when the variables change. If they are missing, the
build stops with a clear error and a warning explaining how to set them:

```bash
WIFI_SSID='my-ssid' WIFI_PASSWORD='my-password' cargo build --release
```

Flash into the board's **UART** port (power switch ON), then open the serial
monitor:

```bash
cargo run --release
# or explicitly:
WIFI_SSID='my-ssid' WIFI_PASSWORD='my-password' \
  espflash flash --chip esp32s31 --monitor \
  target/riscv32imafc-unknown-none-elf/release/weather-advisor
```

If flashing fails with a "Broken pipe" error, run the command again. If it
keeps failing, hold **Boot**, tap **Reset**, then release **Boot** to enter
download mode by hand.

Serial logging follows `ESP_LOG` (baked in at build time, default `info`):
e.g. `ESP_LOG=debug WIFI_SSID=... WIFI_PASSWORD=... cargo build --release`.

Note: like the reference repo, `Cargo.toml` pins the whole esp-rs stack
(esp-hal, esp-rtos, esp-radio, esp-alloc, esp-println, esp-backtrace,
esp-bootloader-esp-idf) to one esp-hal `main` commit via `[patch.crates-io]`,
because released esp-radio has a Wi-Fi-connect hang on the ESP32-S31. The
first build downloads that commit from GitHub.

## LCD

4.3" 800x480 RGB565 LCD subboard (ESP32-S3-LCD-EV-Board-SUB3) on the LCD_CAM
DPI interface, exact reference pin map (also `display::LcdPins` /
`lcd_pins!` in `src/display.rs`):

| Panel signal | ESP32-S31 GPIO |
|--------------|----------------|
| PCLK         | GPIO40         |
| VSYNC        | GPIO45         |
| HSYNC        | GPIO44         |
| DE           | GPIO43         |
| B0–B4 (DB0–4)   | GPIO8, GPIO9, GPIO10, GPIO11, GPIO12 |
| G0–G5 (DB5–10)  | GPIO13, GPIO14, GPIO15, GPIO16, GPIO17, GPIO18 |
| R0–R4 (DB11–15) | GPIO19, GPIO33, GPIO34, GPIO35, GPIO36 |

Panel timing is Espressif's `SUB_BOARD3_800_480_PANEL_35HZ_RGB_TIMING`
(18 MHz pixel clock, ~35 fps); the panel is plain RGB (no init commands).

## Project layout

- `src/bin/main.rs` — startup (heap, PSRAM split, esp-rtos), display init,
  Wi-Fi + embassy-net, refresh loop (fetch → draw → animate → 30 min sleep,
  retry after 30 s on failure).
- `src/bin/display-test.rs` — display hardware test: renders a forced storm
  scene (the heaviest animation load) and animates it forever with
  heartbeats on serial. If the panel shows nothing here, the problem is
  hardware (power, cable, panel), not the app.
- `src/display.rs` — the reference board library's LCD driver, copied
  verbatim: PSRAM frame buffer scanned out by a self-reloading DMA descriptor
  ring; drawing becomes visible after `present()` writes the touched rows
  back from the CPU cache; VSYNC + DMA-position drift detection restarts the
  stream if the panel loses sync.
- `src/weather.rs` — Open-Meteo URLs (plain HTTP; no TLS needed) and minimal
  JSON extraction for `current`: `temperature_2m`, `weather_code`,
  `precipitation`, `uv_index`, `is_day`; city → lat/lon via the geocoding API.
- `src/advice.rs` — outfit/rain/snow/UV rules from the project plan.
- `src/ui.rs` — screen layout with `embedded-graphics`.
- `src/scene.rs` — animated weather backdrop (phone-weather-app style): a static
  scene (sky gradient, sun/moon/stars/clouds, text panel) rendered once into a
  second PSRAM "base" buffer, plus a light particle layer animated on top at
  ~14 fps — rain streaks (drizzle → storm), drifting snow, and lightning in
  thunderstorms. Day/night scenery follows the API's `is_day`. Particles only
  touch small cells per tick and are restored from the base copy, so the
  continuous LCD scan-out (see `src/display.rs`) keeps its PSRAM bandwidth.

Unit tests for parsing, advice bands and formatting run on the host:

```bash
cargo test --lib --target x86_64-unknown-linux-gnu
```

## Weather rules (from the plan)

- Temperature: <0 °C heavy winter coat/gloves/hat · 0–9 warm coat · 10–15
  lighter coat/jacket · 16–21 light jacket or cardigan · >21 light, breathable
  clothes; ≥25 °C adds a hydration/sun-protection reminder.
- Rain/snow: freezing rain → slippery-surfaces warning · snow → warm coat +
  waterproof shoes · heavy rain → umbrella + rain jacket · rain/drizzle or
  precipitation > 0 → umbrella.
- UV index: 0–2 low · 3–5 sunscreen · 6–7 sunscreen, sunglasses, shade ·
  ≥8 strong protection, reduce exposure.
