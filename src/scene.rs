//! Animated weather backdrop in the style of a phone weather app: a static
//! weather-themed scene (sky gradient, sun/moon/stars/clouds, header and
//! text panel) rendered once into a "base" copy of the frame, then a light
//! particle layer (rain streaks, snow, lightning) animated on top.
//!
//! Bandwidth rules (see `display.rs`): the DMA scans the frame buffer out of
//! PSRAM continuously, so per-tick drawing must stay small. Particles only
//! touch small cells per tick; whatever was under them is restored from the
//! base copy, so they can cross the text panel without erasing it.

use core::cmp::{max, min};

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
};
use heapless::Vec as HVec;

use crate::{
    display::{Canvas, DirtyChunks, HEIGHT, WIDTH},
    ui,
    weather::Weather,
};

/// Maximum particles animated at once (rain drops or snow flakes).
const MAX_PARTICLES: usize = 96;
/// Rain streak geometry (width × length in pixels). Kept short so a whole
/// animation tick's write-back (~40 drops × 2 cells × 8 chunks × 64 B ≈
/// 41 KB) fits into the vertical blanking window.
const RAIN_W: usize = 2;
const RAIN_H: usize = 8;
/// Snow flake geometry (square side in pixels).
const SNOW_S: usize = 3;
/// Animation period; ~12.5 ticks per second.
pub const TICK: embassy_time::Duration = embassy_time::Duration::from_millis(80);

/// What goes on top of the panel: a status message or the weather content.
pub enum Overlay<'x> {
    Message(&'x str),
    Weather(&'x Weather),
}

/// What the sky shows.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Boot,
    Error,
    ClearDay,
    ClearNight,
    PartlyDay,
    PartlyNight,
    Overcast,
    Fog,
    LightRain,
    HeavyRain,
    Snow,
    Storm,
}

impl Kind {
    /// Sky gradient: (top, bottom) as (r5, g6, b5) triplets.
    fn sky(self) -> ((u8, u8, u8), (u8, u8, u8)) {
        match self {
            Kind::Boot => ((5, 10, 14), (9, 16, 20)),
            Kind::Error => ((3, 6, 9), (6, 10, 14)),
            Kind::ClearDay => ((4, 18, 26), (10, 36, 31)),
            Kind::ClearNight => ((0, 2, 5), (1, 5, 9)),
            Kind::PartlyDay => ((5, 16, 24), (10, 30, 30)),
            Kind::PartlyNight => ((0, 2, 6), (2, 6, 10)),
            Kind::Overcast => ((6, 10, 13), (10, 16, 17)),
            Kind::Fog => ((8, 12, 13), (11, 15, 16)),
            Kind::LightRain => ((3, 7, 11), (6, 13, 15)),
            Kind::HeavyRain => ((2, 5, 9), (5, 10, 12)),
            Kind::Snow => ((10, 20, 22), (14, 26, 24)),
            Kind::Storm => ((1, 3, 6), (3, 7, 9)),
        }
    }

    fn from_code(code: u16, is_day: bool) -> Kind {
        match code {
            0 | 1 => day_night(is_day, Kind::ClearDay, Kind::ClearNight),
            2 => day_night(is_day, Kind::PartlyDay, Kind::PartlyNight),
            3 => Kind::Overcast,
            45 | 48 => Kind::Fog,
            51..=57 | 61 => Kind::LightRain,
            63..=67 | 80 | 81 => Kind::HeavyRain,
            82 | 95..=99 => Kind::Storm,
            71..=77 | 85 | 86 => Kind::Snow,
            _ => Kind::Overcast,
        }
    }

    fn is_night(self) -> bool {
        matches!(self, Kind::ClearNight | Kind::PartlyNight)
    }

    /// Number of decorative clouds.
    fn clouds(self) -> usize {
        match self {
            Kind::Boot | Kind::Error | Kind::ClearDay | Kind::ClearNight => 0,
            Kind::PartlyDay | Kind::PartlyNight => 2,
            Kind::Overcast | Kind::Fog => 4,
            Kind::LightRain | Kind::HeavyRain => 3,
            Kind::Snow | Kind::Storm => 4,
        }
    }

    fn has_sun(self) -> bool {
        matches!(self, Kind::ClearDay | Kind::PartlyDay)
    }

    /// Particle colour, `None` for static scenes.
    fn particle_color(self) -> Option<Rgb565> {
        match self {
            Kind::LightRain | Kind::HeavyRain | Kind::Storm => Some(Rgb565::new(20, 48, 28)),
            Kind::Snow => Some(Rgb565::WHITE),
            _ => None,
        }
    }

    fn is_snow(self) -> bool {
        self == Kind::Snow
    }
}

fn day_night(is_day: bool, day: Kind, night: Kind) -> Kind {
    if is_day {
        day
    } else {
        night
    }
}

/// A falling particle: position and vertical speed (px per tick).
#[derive(Clone, Copy)]
struct Particle {
    x: i32,
    y: i32,
    v: i32,
}

/// An animated weather scene. The static scene is rendered OFF-SCREEN into
/// the `base` buffer; [`Scene::reveal_step`] then copies it onto the panel a
/// band at a time (each step's write-back fits into the vertical blanking
/// window, so the scan-out stream is never disturbed and never needs a
/// restart — a mid-run restart is not reliable on this board). Particles are
/// animated on top of the revealed scene, restoring whatever was under them
/// from the same base buffer.
pub struct Scene<'a> {
    kind: Kind,
    /// Off-screen copy of the fully rendered scene; also the source for
    /// restoring the area under moving particles.
    base: &'a mut [u8],
    /// Scratch dirty map for off-screen rendering into `base`.
    scratch: DirtyChunks,
    particles: HVec<Particle, MAX_PARTICLES>,
    rng: u32,
    tick: u32,
    /// Lightning bolt being flashed: (x, y, ticks left).
    bolt: Option<(i32, i32, u8)>,
}

/// Number of reveal steps: each copies `HEIGHT / REVEAL_STEPS` rows
/// (~48 KB) onto the panel per frame, inside the blanking window.
pub const REVEAL_STEPS: usize = 16;

impl<'a> Scene<'a> {
    /// Builds the scene for `weather`, or a neutral boot scene when `None`.
    /// `base` must be a full frame-sized buffer in PSRAM that stays valid for
    /// the scene's lifetime.
    pub fn new(weather: Option<&Weather>, base: &'a mut [u8], seed: u32) -> Self {
        let kind = match weather {
            None => Kind::Boot,
            Some(w) => Kind::from_code(w.code, w.is_day),
        };
        Self::with_kind(kind, base, seed)
    }

    /// Neutral static error scene.
    pub fn error(base: &'a mut [u8], seed: u32) -> Self {
        Self::with_kind(Kind::Error, base, seed)
    }

    fn with_kind(kind: Kind, base: &'a mut [u8], seed: u32) -> Self {
        let base = &mut base[..crate::display::FRAME_BYTES];
        let mut scene = Scene {
            kind,
            base,
            scratch: crate::display::DirtyChunks::new(),
            particles: HVec::new(),
            rng: seed | 1,
            tick: 0,
            bolt: None,
        };
        let count = match kind.particle_color() {
            Some(_) if kind == Kind::HeavyRain || kind == Kind::Storm => 32,
            Some(_) if kind.is_snow() => 40,
            Some(_) => 32,
            None => 0,
        };
        for _ in 0..count {
            let speed = if kind.is_snow() {
                scene.rand(2) + 1
            } else {
                scene.rand(9) + 8
            };
            let x = scene.rand(WIDTH as i32);
            let y = scene.rand(HEIGHT as i32);
            let _ = scene.particles.push(Particle { x, y, v: speed });
        }
        scene
    }

    fn rand(&mut self, range: i32) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % range.max(1) as u32) as i32
    }

    /// Whether the scene has anything to animate.
    pub fn animated(&self) -> bool {
        !self.particles.is_empty() || self.kind == Kind::Storm
    }

    /// Renders the whole static scene into the off-screen base buffer.
    pub fn render(&mut self, city: &str, overlay: Overlay<'_>) {
        let kind = self.kind;
        let seed = self.rng;
        let base: &mut [u8] = &mut *self.base;
        let dirty = &mut self.scratch;
        let mut canvas = crate::display::Canvas::from_parts(base, dirty);
        Self::draw_sky(kind, &mut canvas);
        Self::draw_decorations(kind, seed, &mut canvas);
        ui::draw_panel(&mut canvas, city);
        match overlay {
            Overlay::Message(message) => {
                if kind == Kind::Boot {
                    ui::draw_boot_message(&mut canvas, message);
                } else {
                    ui::draw_error_texts(&mut canvas, message);
                }
            }
            Overlay::Weather(w) => ui::draw_weather_content(&mut canvas, w),
        }
    }

    /// Copies the `step`-th horizontal band of the rendered scene onto the
    /// panel (call once per frame for `step` in `0..REVEAL_STEPS`, then
    /// `present()`); the band's write-back fits into the blanking window.
    pub fn reveal_step(&mut self, canvas: &mut Canvas<'_>, step: usize) {
        let band = (HEIGHT + REVEAL_STEPS - 1) / REVEAL_STEPS;
        let y = step * band;
        canvas.restore(self.base, 0, y, WIDTH, band);
    }

    fn draw_sky(kind: Kind, canvas: &mut Canvas<'_>) {
        let (top, bottom) = kind.sky();
        let (tr, tg, tb) = top;
        let (br, bg, bb) = bottom;
        let top = Rgb565::new(tr, tg, tb);
        let bottom = Rgb565::new(br, bg, bb);
        for y in 0..HEIGHT {
            let t = y as u32 * 255 / (HEIGHT - 1) as u32;
            canvas
                .fill_solid(
                    &Rectangle::new(Point::new(0, y as i32), Size::new(WIDTH as u32, 1)),
                    mix(top, bottom, t as u16),
                )
                .ok();
        }
    }

    fn draw_decorations(kind: Kind, seed: u32, canvas: &mut Canvas<'_>) {
        if kind.has_sun() {
            Circle::new(Point::new(640, 105), 40)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::new(31, 62, 8)))
                .draw(canvas)
                .ok();
            Circle::new(Point::new(640, 105), 52)
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(28, 56, 12), 3))
                .draw(canvas)
                .ok();
        }
        if kind.is_night() {
            // Moon: pale disc with an offset bite taken out of it.
            Circle::new(Point::new(650, 100), 32)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::new(28, 60, 24)))
                .draw(canvas)
                .ok();
            let (sr, sg, sb) = kind.sky().0;
            Circle::new(Point::new(638, 92), 30)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::new(sr, sg, sb)))
                .draw(canvas)
                .ok();
            // Stars: small dots scattered over the upper sky.
            let mut rng = seed;
            for _ in 0..40 {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let x = (rng % (WIDTH as u32 - 20)) as i32 + 10;
                let y = ((rng >> 8) % 280) as i32 + 10;
                let s = (rng >> 16) % 3 + 1;
                canvas
                    .fill_solid(
                        &Rectangle::new(Point::new(x, y), Size::new(s as u32, s as u32)),
                        Rgb565::new(24, 56, 26),
                    )
                    .ok();
            }
        }
        let cloud_color = if kind.is_night() {
            Rgb565::new(4, 9, 12)
        } else {
            Rgb565::new(17, 34, 26)
        };
        for i in 0..kind.clouds() {
            let x = 90 + (i as i32) * 190;
            let y = 90 + ((i % 2) as i32) * 55;
            draw_cloud(canvas, x, y, cloud_color);
        }
    }

    /// One animation step: erase + advance + redraw every particle, flash
    /// lightning in storm scenes. Call every [`TICK`], then `present()`.
    pub fn tick(&mut self, canvas: &mut Canvas<'_>) {
        self.tick += 1;
        let color = match self.kind.particle_color() {
            Some(color) => color,
            None => return,
        };
        let snow = self.kind.is_snow();
        let storm = self.kind == Kind::Storm;
        let (w, h) = if snow {
            (SNOW_S, SNOW_S)
        } else {
            (RAIN_W, RAIN_H)
        };

        for i in 0..self.particles.len() {
            let mut p = self.particles[i];
            let (x, y) = (max(p.x, 0) as usize, max(p.y, 0) as usize);
            canvas.restore(self.base, x, y, w, h);

            p.y += p.v;
            if !snow && self.tick % 3 == 0 {
                p.x -= if storm { 2 } else { 1 };
            }
            if snow {
                p.x += self.rand(3) - 1;
            }
            if p.y > HEIGHT as i32 {
                p.y = -(h as i32);
                p.x = self.rand(WIDTH as i32);
            }
            if p.x < -4 {
                p.x = WIDTH as i32 - 4;
            }
            self.particles[i] = p;

            let (dx, dy) = (max(p.x, 0) as usize, max(p.y, 0) as usize);
            if dx < WIDTH && dy < HEIGHT {
                canvas
                    .fill_solid(
                        &Rectangle::new(
                            Point::new(dx as i32, dy as i32),
                            Size::new(w as u32, h as u32),
                        ),
                        color,
                    )
                    .ok();
            }
        }

        // Lightning: draw over everything for two ticks, then restore.
        if let Some((bx, by, left)) = self.bolt {
            if left == 0 {
                canvas.restore(
                    self.base,
                    bx.saturating_sub(30).max(0) as usize,
                    by.max(0) as usize,
                    60,
                    150,
                );
                self.bolt = None;
            } else {
                draw_bolt(canvas, bx, by);
                self.bolt = Some((bx, by, left - 1));
            }
        } else if storm && self.tick % 50 == 25 {
            self.bolt = Some((self.rand(600) + 100, 60, 2));
        }
    }
}

/// Linear interpolation between two colours, `t` in 0..=255.
fn mix(a: Rgb565, b: Rgb565, t: u16) -> Rgb565 {
    let (ar, ag, ab) = channels(a);
    let (br, bg, bb) = channels(b);
    let (ar, ag, ab) = (ar as i32, ag as i32, ab as i32);
    let (br, bg, bb) = (br as i32, bg as i32, bb as i32);
    let t = t as i32;
    Rgb565::new(
        (ar + (br - ar) * t / 255) as u8,
        (ag + (bg - ag) * t / 255) as u8,
        (ab + (bb - ab) * t / 255) as u8,
    )
}

fn channels(c: Rgb565) -> (u8, u8, u8) {
    let raw = c.into_storage();
    (
        ((raw >> 11) & 0x1f) as u8,
        ((raw >> 5) & 0x3f) as u8,
        (raw & 0x1f) as u8,
    )
}

/// Soft cloud from overlapping filled discs.
fn draw_cloud(canvas: &mut Canvas<'_>, x: i32, y: i32, color: Rgb565) {
    for (dx, dy, r) in [(0, 0, 34), (36, 10, 24), (-34, 8, 22), (12, 14, 26)] {
        Circle::new(Point::new(x + dx, y + dy), r as u32)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(canvas)
            .ok();
    }
}

/// Jagged lightning bolt from `(x, y)` downward.
fn draw_bolt(canvas: &mut Canvas<'_>, x: i32, y: i32) {
    let mut px = x;
    let mut py = y;
    for (i, dy) in [18, 16, 20, 18, 22, 16, 20].into_iter().enumerate() {
        let nx = px + if i % 2 == 0 { 9 } else { -11 };
        let ny = py + dy;
        let (x0, x1) = (min(px, nx), max(px, nx));
        canvas
            .fill_solid(
                &Rectangle::new(
                    Point::new(x0, py),
                    Size::new((x1 - x0 + 4) as u32, dy as u32),
                ),
                Rgb565::YELLOW,
            )
            .ok();
        px = nx;
        py = ny;
    }
}
