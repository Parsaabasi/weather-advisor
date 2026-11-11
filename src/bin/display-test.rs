#![no_std]
#![no_main]

// Display hardware test: fills the whole panel with a solid colour that
// changes every two seconds (red, green, blue, white). If the panel shows
// nothing in this mode, the problem is hardware (power, cable, panel), not
// the app.

use embedded_graphics::{
    pixelcolor::{Rgb565, raw::RawU16},
    prelude::*,
};
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    peripherals::PSRAM,
    psram::Psram,
    timer::timg::TimerGroup,
};
use esp_println::println;
use weather_advisor::display::Display;

esp_bootloader_esp_idf::esp_app_desc!();

const COLORS: [(u8, u8, u8); 4] = [
    (31, 0, 0),    // red
    (0, 63, 0),    // green
    (0, 0, 31),    // blue
    (31, 63, 31),  // white
];

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    println!("display test: starting");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 96 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let framebuffer = psram_split(peripherals.PSRAM, weather_advisor::display::FRAME_BYTES);

    let mut display = match Display::new(
        peripherals.LCD_CAM,
        peripherals.DMA_AXI_CH0,
        weather_advisor::lcd_pins!(peripherals),
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

    println!("display init ok, cycling colours");
    loop {
        for color in COLORS {
            {
                let raw = RawU16::from(Rgb565::new(color.0, color.1, color.2)).into_inner();
                let bytes = raw.to_le_bytes();
                for row in display.framebuffer().chunks_exact_mut(2) {
                    row.copy_from_slice(&bytes);
                }
            }
            println!("showing colour R{} G{} B{}", color.0, color.1, color.2);
            let _ = display.present().await;
            Timer::after_secs(2).await;
        }
    }
}

fn psram_split(psram: PSRAM<'static>, reserve: usize) -> &'static mut [u8] {
    let psram = Psram::new(psram, Default::default());
    let (start, size) = psram.raw_parts();
    let reserve = reserve.next_multiple_of(64).min(size);
    unsafe {
        if size > reserve {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                start.add(reserve),
                size - reserve,
                esp_alloc::MemoryCapability::External.into(),
            ));
        }
        core::slice::from_raw_parts_mut(start, reserve)
    }
}
