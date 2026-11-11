#![no_std]
#![no_main]

// Display hardware test: renders a forced storm scene (the heaviest
// animation load: dense rain + lightning) and animates it forever, printing
// a heartbeat every 10 s. Purpose: verify on serial that the display driver
// reports no stream drift under the worst-case animation traffic. If the
// panel shows nothing in this mode, the problem is hardware (power, cable,
// panel), not the app.

use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    peripherals::PSRAM,
    psram::Psram,
    timer::timg::TimerGroup,
};
use esp_println::println;
use weather_advisor::{display, lcd_pins, scene, weather::Weather};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    println!("display test: starting");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 96 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let (framebuffer, base) = psram_split(peripherals.PSRAM, display::FRAME_BYTES);

    let mut display = match display::Display::new(
        peripherals.LCD_CAM,
        peripherals.DMA_AXI_CH0,
        lcd_pins!(peripherals),
        framebuffer,
    ) {
        Ok(display) => display,
        Err(e) => {
            println!("display init failed: {e}");
            loop {
                Timer::after_secs(60).await;
            }
        }
    };
    println!("display init ok");

    // Force the heaviest animated scene (storm: dense rain + lightning).
    let storm = Weather {
        temp_tenths: 47,
        code: 95,
        precip_tenths: 90,
        uv_tenths: 0,
        is_day: true,
    };
    let mut scene = scene::Scene::new(Some(&storm), base, 0x1234_5678);
    scene.render("TEST", scene::Overlay::Weather(&storm));
    println!("storm scene rendered, revealing");
    for step in 0..scene::REVEAL_STEPS {
        {
            let mut canvas = display.canvas();
            scene.reveal_step(&mut canvas, step);
        }
        let _ = display.present().await;
    }
    println!("revealed, animating");

    let mut frames = 0u32;
    loop {
        frames += 1;
        // Tick the scene on every third VSYNC (~12 fps); present every frame,
        // as the driver's drift detection expects VSYNC-paced calls.
        if frames % 3 == 0 {
            let mut canvas = display.canvas();
            scene.tick(&mut canvas);
        }
        let result = display.present().await;
        match result {
            Ok(()) if frames % 105 == 0 => {
                let (pos, reference) = display.debug_pos();
                println!("frame {frames}: present ok, dma pos {pos}, ref {reference:?}");
                println!("heartbeat {} (3 s of animation)", frames / 105);
            }
            Err(e) => println!("frame {frames}: present FAILED: {e:?}"),
            Ok(()) => {}
        }
    }
}

/// Maps the 16 MB PSRAM and splits it into the LCD frame buffer and a second
/// full-frame "base" buffer for scene animation; the rest goes to the
/// esp-alloc heap.
fn psram_split(
    psram: PSRAM<'static>,
    frame_bytes: usize,
) -> (&'static mut [u8], &'static mut [u8]) {
    let psram = Psram::new(psram, Default::default());
    let (start, size) = psram.raw_parts();
    let total = (2 * frame_bytes).next_multiple_of(64).min(size);
    println!("psram: start={:p} size={size}", start);
    unsafe {
        if size > total {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                start.add(total),
                size - total,
                esp_alloc::MemoryCapability::External.into(),
            ));
        }
        core::slice::from_raw_parts_mut(start, total).split_at_mut(frame_bytes)
    }
}
