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

/// Centered status message used by the boot screen, scaled 2x.
pub fn draw_boot_message<C: DrawTarget<Color = Rgb565>>(canvas: &mut C, message: &str) {
    let w = message.len() as i32 * 20;
    draw_text_scaled(
        canvas,
        message,
        Point::new(PANEL_WIDTH as i32 / 2 - w / 2, 130),
        WHITE,
        2,
    );
}

/// Error headline, detail and retry note, centered, scaled 2x.
pub fn draw_error_texts<C: DrawTarget<Color = Rgb565>>(canvas: &mut C, message: &str) {
    let headline = "Connection problem";
    draw_text_scaled(
        canvas,
        headline,
        Point::new(PANEL_WIDTH as i32 / 2 - headline.len() as i32 * 10, 110),
        RED,
        2,
    );
    draw_text_scaled(
        canvas,
        message,
        Point::new(PANEL_WIDTH as i32 / 2 - message.len() as i32 * 10, 170),
        WHITE,
        2,
    );
    let retry = "Retrying...";
    draw_text_scaled(
        canvas,
        retry,
        Point::new(PANEL_WIDTH as i32 / 2 - retry.len() as i32 * 10, 240),
        YELLOW,
        2,
    );
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
    let x = 24;

    // Hero temperature, 3x.
    draw_text_scaled(canvas, &fmt_temp(weather), Point::new(x, 84), CYAN, 3);

    // Conditions, 2x.
    draw_text_scaled(
        canvas,
        describe_code(weather.code),
        Point::new(x, 164),
        WHITE,
        2,
    );

    // Outfit advice, 2x with a colored label.
    draw_labeled_scaled(
        canvas,
        x,
        224,
        "Outfit:",
        YELLOW,
        advice::outfit(weather.temp_tenths),
        WHITE,
    );

    // Rain / snow alert or all-clear, 2x.
    match advice::rain_snow(weather) {
        Some(rain) => {
            draw_labeled_scaled(canvas, x, 284, "Alert:", RED, rain, WHITE);
        }
        None => {
            draw_labeled_scaled(canvas, x, 284, "Alert:", YELLOW, "No rain now", WHITE);
        }
    }

    // Rain amount and UV index side by side, 2x.
    draw_text_scaled(canvas, &fmt_precip(weather), Point::new(x, 344), WHITE, 2);
    let mut uv = heapless::String::<16>::new();
    let _ = core::write!(
        uv,
        "{}.{}",
        weather.uv_tenths / 10,
        weather.uv_tenths % 10
    );
    draw_labeled_scaled(canvas, 460, 344, "UV:", YELLOW, &uv, WHITE);

    // Bottom row: sun-protection guidance. When it is hot, the hot-weather
    // reminder takes this slot (it already includes sun protection).
    if let Some(hot) = advice::hot_reminder(weather.temp_tenths) {
        draw_text_scaled(canvas, hot, Point::new(x, 404), ORANGE, 2);
    } else {
        draw_text_scaled(canvas, advice::uv(weather.uv_tenths), Point::new(x, 404), WHITE, 2);
    }
}

/// Draws a `label` and a `value` on one line, both scaled 2x.
fn draw_labeled_scaled<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    x: i32,
    y: i32,
    label: &str,
    label_color: Rgb565,
    value: &str,
    value_color: Rgb565,
) {
    draw_text_scaled(canvas, label, Point::new(x, y), label_color, 2);
    let value_x = x + (label.len() as i32 + 1) * 20;
    draw_text_scaled(canvas, value, Point::new(value_x, y), value_color, 2);
}

/// Draws `text` with the 10x20 font scaled by `scale` (nearest neighbor):
/// the text is rendered into a 1-bit-per-pixel scratch bitmap, then blitted
/// as `scale`-sized blocks.
fn draw_text_scaled<C: DrawTarget<Color = Rgb565>>(
    canvas: &mut C,
    text: &str,
    pos: Point,
    color: Rgb565,
    scale: u32,
) {
    if scale <= 1 {
        Text::new(text, pos, style(color)).draw(canvas).ok();
        return;
    }
    const CHAR_W: usize = 10;
    const CHAR_H: usize = 20;
    // The bitmap is taller than the font cell and the text is drawn at its
    // BASELINE row: eg positions text by the baseline, so drawing at y = 0
    // would put every glyph above the bitmap and only periods/hyphens
    // (which sit at or below the baseline) would survive.
    const BITMAP_H: usize = CHAR_H + 4;
    let baseline = FONT_10X20.baseline as usize;
    let w = text.len() * CHAR_W;
    let h = BITMAP_H;
    let stride = w.div_ceil(8);
    let mut bits = alloc::vec![0u8; stride * h];

    struct Bitmap<'a> {
        bits: &'a mut [u8],
        stride: usize,
        w: usize,
        h: usize,
    }
    impl Dimensions for Bitmap<'_> {
        fn bounding_box(&self) -> Rectangle {
            Rectangle::new(Point::zero(), Size::new(self.w as u32, self.h as u32))
        }
    }
    impl DrawTarget for Bitmap<'_> {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(p, _c) in pixels {
                if p.x >= 0 && p.y >= 0 {
                    let (px, py) = (p.x as usize, p.y as usize);
                    if px < self.w && py < self.h {
                        self.bits[py * self.stride + px / 8] |= 1 << (px % 8);
                    }
                }
            }
            Ok(())
        }
    }

    Text::new(text, Point::new(0, baseline as i32), style(color))
        .draw(&mut Bitmap {
            bits: &mut bits,
            stride,
            w,
            h,
        })
        .ok();

    for py in 0..h {
        for px in 0..w {
            if bits[py * stride + px / 8] & (1 << (px % 8)) != 0 {
                let p = pos
                    + Point::new(
                        (px * scale as usize) as i32,
                        (py * scale as usize) as i32,
                    );
                canvas
                    .fill_solid(
                        &Rectangle::new(p, Size::new(scale, scale)),
                        color,
                    )
                    .ok();
            }
        }
    }
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
#[cfg(test)]
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
