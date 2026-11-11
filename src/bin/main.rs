#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_net::{
    Runner, StackResources,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    peripherals::PSRAM,
    psram::Psram,
    timer::timg::TimerGroup,
};
use esp_println::logger::init_logger_from_env;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, PowerSaveMode,
    WifiController, sta::StationConfig,
};
use reqwless::client::HttpClient;
use reqwless::request::{Method, RequestBuilder};
use weather_advisor::{display, lcd_pins, scene, weather};

extern crate alloc;

use alloc::format;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Wi-Fi credentials and location are compile-time configuration; pass them
/// at build time, e.g.
/// `WIFI_SSID=... WIFI_PASSWORD=... cargo build --release`.
// const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_SSID: &str = "EspressifSystems";
// const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
const WIFI_PASSWORD: &str = "Espressif32";
const CITY: &str = match option_env!("WEATHER_CITY") {
    Some(city) if !city.is_empty() => city,
    _ => "Brno",
};

/// Refresh period from the plan: every 30 minutes, board always on.
const REFRESH_PERIOD: Duration = Duration::from_secs(30 * 60);
/// Retry delay after a failed fetch.
const RETRY_PERIOD: Duration = Duration::from_secs(30);
/// Cap on a single HTTP exchange, so a stalled server cannot hang the loop.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Internal heap size; the Wi-Fi driver needs about 60 KiB of internal RAM
/// (same value as the reference Wi-Fi demo).
const INTERNAL_HEAP: usize = 96 * 1024;
macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // Internal heap, plain RAM (the reclaimed bootloader RAM region is not
    // usable for the heap on this chip, see the reference board docs).
    esp_alloc::heap_allocator!(size: INTERNAL_HEAP);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // --- Display: map PSRAM, reserve the frame + scene base buffers ---
    let mut base;
    let framebuffer;
    {
        let (fb, scene_base) = psram_split(peripherals.PSRAM, display::FRAME_BYTES);
        framebuffer = fb;
        base = scene_base;
    }
    let rng = esp_hal::rng::Rng::new();
    let mut display = match display::Display::new(
        peripherals.LCD_CAM,
        peripherals.DMA_AXI_CH0,
        lcd_pins!(peripherals),
        framebuffer,
    ) {
        Ok(display) => display,
        Err(e) => {
            log::error!("display init failed: {e}");
            loop {
                Timer::after_secs(60).await;
            }
        }
    };

    {
        let mut scene = scene::Scene::new(None, &mut base, rng.random());
        scene.draw(
            &mut display,
            CITY,
            scene::Overlay::Message("Starting up..."),
        );
    }
    let _ = display.present().await;

    // --- Wi-Fi + network stack ---
    log::info!("Starting Wi-Fi ({WIFI_SSID})");
    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(WIFI_SSID.try_into().unwrap())
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                WIFI_PASSWORD.try_into().unwrap(),
            )),
    );
    let wifi_interface = Interface::station();
    let mut controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("Failed to initialize Wi-Fi controller");
    // Board always on: keep the radio fully powered.
    controller.set_power_saving(PowerSaveMode::None).unwrap();

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    spawner.spawn(connection_task(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;
    log::info!("Network up");

    // --- Refresh loop: fetch + draw, retry on failure, repeat forever ---
    let mut lat_lon: Option<weather::Location> = None;
    loop {
        // Each pass is one attempt, capped by FETCH_TIMEOUT: on failure the
        // screen says so, the board waits RETRY_PERIOD and tries again.
        let outcome = match lat_lon {
            None => embassy_time::with_timeout(FETCH_TIMEOUT, geocode(stack))
                .await
                .map(|r| r.map(Step::Location)),
            Some(location) => {
                embassy_time::with_timeout(FETCH_TIMEOUT, fetch_forecast(stack, location))
                    .await
                    .map(|r| r.map(Step::Weather))
            }
        };
        match outcome {
            Ok(Ok(Step::Location(location))) => {
                lat_lon = Some(location);
                log::info!("geocoded city: lat {} lon {}", location.lat_micro as f32 / 1e6, location.lon_micro as f32 / 1e6);
                // Coordinates resolved: fetch the forecast right away.
                continue;
            }
            Ok(Ok(Step::Weather(w))) => {
                log::info!(
                    "weather: {} C, code {}, rain {}, uv {}",
                    w.temp_tenths as f32 / 10.0,
                    w.code,
                    w.precip_tenths as f32 / 10.0,
                    w.uv_tenths as f32 / 10.0
                );
                let mut scene = scene::Scene::new(Some(&w), &mut base, rng.random());
                scene.draw(&mut display, CITY, scene::Overlay::Weather(&w));
                let _ = display.present().await;
                animate_for(&mut display, &mut scene, REFRESH_PERIOD).await;
            }
            Ok(Err(e)) => {
                show_failure(&mut display, &mut base, rng.random(), &format!("Fetch failed: {e:?}"))
                    .await;
            }
            Err(_) => {
                show_failure(&mut display, &mut base, rng.random(), "Request timed out").await;
            }
        }
    }
}

/// What one refresh pass should do next.
enum Step {
    Location(weather::Location),
    Weather(weather::Weather),
}

/// Animates the scene (rain/snow/lightning) until `total` has elapsed, then
/// returns so the caller can refresh the weather.
///
/// `present()` runs on EVERY frame (VSYNC-paced), as the display driver's
/// drift detection expects; the scene is ticked only on every third frame
/// (~12 fps) to keep the per-tick PSRAM traffic inside the blanking window.
async fn animate_for(
    display: &mut display::Display<'_>,
    scene: &mut scene::Scene<'_>,
    total: Duration,
) {
    let deadline = Instant::now() + total;
    let mut frames: u32 = 0;
    while Instant::now() < deadline {
        frames += 1;
        if scene.animated() && frames % 3 == 0 {
            let mut canvas = display.canvas();
            scene.tick(&mut canvas);
        }
        let _ = display.present().await;
    }
}

/// Draws the failure scene (so partial draws never become visible), then
/// waits out the retry period.
async fn show_failure(
    display: &mut display::Display<'_>,
    base: &mut &'static mut [u8],
    seed: u32,
    message: &str,
) {
    log::warn!("{message}");
    let mut scene = scene::Scene::error(base, seed);
    scene.draw(display, CITY, scene::Overlay::Message(message));
    let _ = display.present().await;
    Timer::after(RETRY_PERIOD).await;
}

/// Resolves the configured city name to coordinates.
async fn geocode(
    stack: embassy_net::Stack<'_>,
) -> Result<weather::Location, reqwless::Error> {
    let body = http_get(stack, &weather::geocoding_url(CITY)).await?;
    let text = core::str::from_utf8(&body).map_err(|_| reqwless::Error::Codec)?;
    weather::parse_geocoding(text).ok_or(reqwless::Error::Codec)
}

/// Fetches the current weather for `location`.
async fn fetch_forecast(
    stack: embassy_net::Stack<'_>,
    location: weather::Location,
) -> Result<weather::Weather, reqwless::Error> {
    let body = http_get(stack, &weather::forecast_url(location)).await?;
    let text = core::str::from_utf8(&body).map_err(|_| reqwless::Error::Codec)?;
    weather::parse_forecast(text).ok_or(reqwless::Error::Codec)
}

/// One plain-HTTP GET, body capped at [`RX_BUF_LEN`] bytes.
const RX_BUF_LEN: usize = 8 * 1024;

async fn http_get(
    stack: embassy_net::Stack<'_>,
    url: &str,
) -> Result<alloc::vec::Vec<u8>, reqwless::Error> {
    const TCP_STATE: TcpClientState<1, RX_BUF_LEN, 1024> = TcpClientState::new();
    let state = &TCP_STATE;
    let tcp_client = TcpClient::new(stack, state);
    let dns_client = DnsSocket::new(stack);

    let mut client = HttpClient::new(&tcp_client, &dns_client);
    let mut rx_buf = [0u8; RX_BUF_LEN];

    // No manual `Host` header: reqwless already writes one from the URL, and
    // a duplicate makes servers answer 400 Bad Request.
    let headers = [("Connection", "close")];
    let request = client.request(Method::GET, url).await?;
    let mut request = request.headers(&headers);
    let response = request.send(&mut rx_buf).await?;
    if !response.status.is_successful() {
        log::warn!("HTTP status {}", response.status.0);
    }
    let body = response.body().read_to_end().await?;
    Ok(alloc::vec::Vec::from(body))
}

/// Maps the 16 MB PSRAM and splits it into the LCD frame buffer and a second
/// full-frame "base" buffer for scene animation; the rest goes to the
/// esp-alloc heap. Copied from the reference board library
/// (`esp32-s31-korvo/src/board.rs`).
fn psram_split(
    psram: PSRAM<'static>,
    frame_bytes: usize,
) -> (&'static mut [u8], &'static mut [u8]) {
    let psram = Psram::new(psram, Default::default());
    let (start, size) = psram.raw_parts();
    let total = (2 * frame_bytes).next_multiple_of(64).min(size);

    // SAFETY: the PSRAM mapping is valid for `size` bytes for the rest of the
    // program, nothing else uses it, and the heap only gets the part after
    // `total`.
    unsafe {
        if size > total {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                start.add(total),
                size - total,
                esp_alloc::MemoryCapability::External.into(),
            ));
        }
        let all = core::slice::from_raw_parts_mut(start, total);
        all.split_at_mut(frame_bytes)
    }
}

/// Reconnects whenever the station drops; runs forever.
#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        log::info!("Wi-Fi connecting...");
        match controller.connect_async().await {
            Ok(info) => {
                log::info!("Wi-Fi connected: {info:?}");
                let info = controller.wait_for_disconnect_async().await.ok();
                log::warn!("Wi-Fi disconnected: {info:?}");
            }
            Err(e) => {
                log::warn!("Wi-Fi connect failed: {e:?}");
            }
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}
