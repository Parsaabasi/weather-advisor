//! Screen layout: renders the weather and advice lines onto the display.
//!
//! Generic over the draw target so the text/number helpers can be unit-tested
//! on the host; the device draws into the LCD frame-buffer canvas.

use core::fmt::Write as _;

use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

use crate::{advice, weather::{Weather, describe_code}};

/// Panel geometry (mirrors `display::WIDTH`/`HEIGHT`).
const PANEL_WIDTH: u32 = 800;
const PANEL_HEIGHT: u32 = 480;

const BACKGROUND: Rgb565 = Rgb565::new(1, 3, 5);
const PANEL: Rgb565 = Rgb565::new(6, 14, 16);
const WHITE: Rgb565 = Rgb565::WHITE;
const HEADER: Rgb565 = Rgb565::new(0, 12, 20);
const YELLOW: Rgb565 = Rgb565::YELLOW;
const CYAN: Rgb565 = Rgb565::CYAN;
const ORANGE: Rgb565 = Rgb565::new(31, 40, 0);
const RED: Rgb565 = Rgb565::RED;

const LINE: i32 = 34;

fn style(color: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    MonoTextStyle::new(&FONT_10X20, color)
}

/// Draws the idle/boot screen.
pub fn draw_boot<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    city: &str,
    message: &str,
) {
    draw_common(canvas, city);
    draw_boot_message(canvas, message);
}

/// Draws the error/retry screen.
pub fn draw_error<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    city: &str,
    message: &str,
) {
    draw_common(canvas, city);
    draw_error_texts(canvas, message);
}

/// Centered status message used by the boot screen.
pub fn draw_boot_message<C: DrawTarget<Color = Rgb565>>(canvas: &mut C, message: &str) {
    Text::with_alignment(
        message,
        Point::new(PANEL_WIDTH as i32 / 2, 140),
        style(WHITE),
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
}

/// Error headline, detail and retry note, centered.
pub fn draw_error_texts<C: DrawTarget<Color = Rgb565>>(canvas: &mut C, message: &str) {
    Text::with_alignment(
        "Connection problem",
        Point::new(PANEL_WIDTH as i32 / 2, 120),
        style(RED),
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
    Text::with_alignment(
        message,
        Point::new(PANEL_WIDTH as i32 / 2, 160),
        style(WHITE),
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
    Text::with_alignment(
        "Retrying...",
        Point::new(PANEL_WIDTH as i32 / 2, 220),
        style(YELLOW),
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
}

/// Draws the full advice screen for `weather`.
pub fn draw_weather<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    city: &str,
    weather: &Weather,
) {
    draw_common(canvas, city);
    draw_weather_content(canvas, weather);
}

/// Weather and advice lines, over an existing background.
pub fn draw_weather_content<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    weather: &Weather,
) {
    let x = 20;
    let mut y = 100;

    // Temperature and conditions.
    Text::new(&fmt_temp(weather), Point::new(x, y), style(CYAN))
        .draw(canvas)
        .ok();
    Text::new(
        describe_code(weather.code),
        Point::new(x + 340, y),
        style(WHITE),
    )
    .draw(canvas)
    .ok();
    y += LINE + 10;

    // Precipitation.
    Text::new(&fmt_precip(weather), Point::new(x, y), style(WHITE))
        .draw(canvas)
        .ok();
    y += LINE + 20;

    // Outfit advice.
    label(canvas, x, y, "Outfit:", YELLOW);
    Text::new(
        advice::outfit(weather.temp_tenths),
        Point::new(x + 110, y),
        style(WHITE),
    )
    .draw(canvas)
    .ok();
    y += LINE;

    // Hot weather reminder.
    if let Some(hot) = advice::hot_reminder(weather.temp_tenths) {
        Text::new(hot, Point::new(x + 110, y), style(ORANGE))
            .draw(canvas)
            .ok();
        y += LINE;
    }

    // Rain / snow alert.
    label(canvas, x, y, "Alert:", YELLOW);
    match advice::rain_snow(weather) {
        Some(rain) => {
            label(canvas, x, y, "Alert:", RED);
            Text::new(rain, Point::new(x + 110, y), style(WHITE))
                .draw(canvas)
                .ok();
        }
        None => {
            Text::new("No rain now", Point::new(x + 110, y), style(WHITE))
                .draw(canvas)
                .ok();
        }
    }
    y += LINE + 20;

    // UV advice.
    label(canvas, x, y, "UV:", YELLOW);
    Text::new(&fmt_uv(weather), Point::new(x + 110, y), style(WHITE))
        .draw(canvas)
        .ok();
    y += LINE;
    Text::new(
        advice::uv(weather.uv_tenths),
        Point::new(x + 110, y),
        style(ORANGE),
    )
    .draw(canvas)
    .ok();
}

fn label<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    x: i32,
    y: i32,
    text: &str,
    color: Rgb565,
) {
    Text::new(text, Point::new(x, y), style(color))
        .draw(canvas)
        .ok();
}

/// Header and background shared by all screens.
fn draw_common<C: DrawTarget<Color = Rgb565>>(canvas: &mut C, city: &str) {
    Rectangle::new(Point::zero(), Size::new(PANEL_WIDTH, PANEL_HEIGHT))
        .into_styled(PrimitiveStyle::with_fill(BACKGROUND))
        .draw(canvas)
        .ok();
    draw_panel(canvas, city);
}

/// Header bar and outlined content panel, over an existing background.
pub fn draw_panel<C: DrawTarget<Color = Rgb565>>(canvas: &mut C, city: &str) {
    Rectangle::new(Point::zero(), Size::new(PANEL_WIDTH, 48))
        .into_styled(PrimitiveStyle::with_fill(HEADER))
        .draw(canvas)
        .ok();
    Text::new("WEATHER & OUTFIT ADVISOR", Point::new(12, 30), style(WHITE))
        .draw(canvas)
        .ok();
    Text::with_alignment(
        city,
        Point::new(PANEL_WIDTH as i32 - 12, 30),
        style(WHITE),
        Alignment::Right,
    )
    .draw(canvas)
    .ok();
    Rectangle::new(
        Point::new(12, 66),
        Size::new(PANEL_WIDTH - 24, PANEL_HEIGHT - 78),
    )
    .into_styled(PrimitiveStyle::with_stroke(PANEL, 1))
    .draw(canvas)
    .ok();
}

/// `Temp: -5.1 C` from tenths of °C.
fn fmt_temp(weather: &Weather) -> heapless::String<24> {
    let mut out = heapless::String::new();
    let tenths = weather.temp_tenths.unsigned_abs();
    let _ = core::write!(
        out,
        "Temp: {}{}.{} C",
        if weather.temp_tenths < 0 { "-" } else { "" },
        tenths / 10,
        tenths % 10
    );
    out
}

/// `Rain: 12.3 mm` from tenths of mm.
fn fmt_precip(weather: &Weather) -> heapless::String<24> {
    let mut out = heapless::String::new();
    let _ = core::write!(
        out,
        "Rain: {}.{} mm",
        weather.precip_tenths / 10,
        weather.precip_tenths % 10
    );
    out
}

/// `UV index: 6.3` from tenths.
fn fmt_uv(weather: &Weather) -> heapless::String<24> {
    let mut out = heapless::String::new();
    let _ = core::write!(
        out,
        "UV index: {}.{}",
        weather.uv_tenths / 10,
        weather.uv_tenths % 10
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather(temp: i16, code: u16, precip: u16, uv: u16) -> Weather {
        Weather {
            temp_tenths: temp,
            code,
            precip_tenths: precip,
            uv_tenths: uv,
            is_day: true,
        }
    }

    #[test]
    fn formats_numbers() {
        let w = weather(-51, 61, 123, 63);
        assert_eq!(fmt_temp(&w).as_str(), "Temp: -5.1 C");
        assert_eq!(fmt_precip(&w).as_str(), "Rain: 12.3 mm");
        assert_eq!(fmt_uv(&w).as_str(), "UV index: 6.3");
        assert_eq!(fmt_temp(&weather(219, 0, 0, 0)).as_str(), "Temp: 21.9 C");
    }
}
