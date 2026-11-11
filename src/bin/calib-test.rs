#![no_std]
#![no_main]

// Display calibration test: draws a labeled pixel ruler (0..800 along the
// top, 0..480 down the left), a border inset by one pixel, big centered text
// and four corner markers — with no Wi-Fi, no radio and no scene code. What
// the panel shows versus what was drawn localises any display error: a
// horizontal shift moves the ruler labels, a stride error shears the
// picture, a vertical error moves the border rows.

use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
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
use weather_advisor::display::{self, Display, HEIGHT, WIDTH};

esp_bootloader_esp_idf::esp_app_desc!();

extern crate alloc;

const WHITE: Rgb565 = Rgb565::WHITE;
const YELLOW: Rgb565 = Rgb565::YELLOW;
const CYAN: Rgb565 = Rgb565::CYAN;
const RED: Rgb565 = Rgb565::RED;
const BG: Rgb565 = Rgb565::new(2, 4, 6);

fn style(color: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    MonoTextStyle::new(&FONT_10X20, color)
}

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    println!("calibration test: starting");
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 96 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let framebuffer = psram_split(peripherals.PSRAM, display::FRAME_BYTES);
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

    {
        let mut canvas = display.canvas();

        // Background.
        Rectangle::new(Point::zero(), Size::new(WIDTH as u32, HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(BG))
            .draw(&mut canvas)
            .ok();

        // Border exactly one pixel inside the panel edges.
        Rectangle::new(Point::new(1, 1), Size::new(WIDTH as u32 - 2, HEIGHT as u32 - 2))
            .into_styled(PrimitiveStyle::with_stroke(WHITE, 1))
            .draw(&mut canvas)
            .ok();

        // Horizontal ruler: 100 px steps, ticks and labels.
        for x in (0..WIDTH).step_by(100) {
            Rectangle::new(Point::new(x as i32, 20), Size::new(1, 24))
                .into_styled(PrimitiveStyle::with_fill(YELLOW))
                .draw(&mut canvas)
                .ok();
            Text::with_alignment(
                &alloc::format!("{}", x),
                Point::new(x as i32, 60),
                style(CYAN),
                Alignment::Center,
            )
            .draw(&mut canvas)
            .ok();
        }

        // Vertical ruler: 100 px steps.
        for y in (0..HEIGHT).step_by(100) {
            Rectangle::new(Point::new(20, y as i32), Size::new(24, 1))
                .into_styled(PrimitiveStyle::with_fill(YELLOW))
                .draw(&mut canvas)
                .ok();
            Text::with_alignment(
                &alloc::format!("{}", y),
                Point::new(56, y as i32 + 8),
                style(CYAN),
                Alignment::Left,
            )
            .draw(&mut canvas)
            .ok();
        }

        // Big centered headline + a row of color chips at known positions.
        Text::with_alignment(
            "DISPLAY CALIBRATION",
            Point::new(WIDTH as i32 / 2, 200),
            style(WHITE),
            Alignment::Center,
        )
        .draw(&mut canvas)
        .ok();
        for (i, color) in [RED, Rgb565::GREEN, Rgb565::BLUE, YELLOW].into_iter().enumerate() {
            Rectangle::new(Point::new(280 + i as i32 * 60, 260), Size::new(40, 40))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(&mut canvas)
                .ok();
        }

        // Bottom-left corner text block.
        Text::new(
            "corner marker at 24,440",
            Point::new(24, 460),
            style(WHITE),
        )
        .draw(&mut canvas)
        .ok();
    }

    let _ = display.present().await;
    println!("calibration screen presented");

    loop {
        Timer::after_secs(10).await;
        println!("heartbeat");
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
