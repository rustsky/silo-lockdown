//! Immediate-mode UI helpers: crisp text at any scale, buttons, word wrap.

use macroquad::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Screen points per logical pixel; used to rasterize text at native resolution.
static SCALE: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0

pub fn set_scale(s: f32) {
    SCALE.store(s.max(0.1).to_bits(), Ordering::Relaxed);
}

/// Every text size the game draws, in logical pixels. Requested sizes snap to the nearest one.
///
/// macroquad's font atlas doubles its texture when it fills up, and text already queued in that
/// frame then samples a deleted texture: every glyph turns to garbage. So the game only ever
/// uses this fixed set of sizes and caches all of them up front (`prewarm_fonts`), and the
/// atlas never has to grow mid-game.
const TEXT_SIZES: [f32; 23] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0,
    30.0, 34.0, 40.0, 44.0, 50.0, 60.0, 120.0,
];
/// Sizes above this are only used for the title, in capitals: cache just those glyphs.
const HEADLINE: f32 = 60.0;

fn snap(size: f32) -> f32 {
    *TEXT_SIZES
        .iter()
        .min_by(|a, b| (*a - size).abs().total_cmp(&(*b - size).abs()))
        .unwrap()
}

/// Raster size (before macroquad's own DPI factor) for logical size `size`.
fn raster(size: f32) -> u16 {
    (snap(size) * scale()).round().clamp(1.0, 400.0) as u16
}

/// Scale and DPI the font cache was last filled for.
static WARMED: AtomicU32 = AtomicU32::new(0);

/// Cache every printable character at every text size for the current scale, once per
/// scale/DPI change, before anything is drawn that frame.
pub fn prewarm_fonts() {
    let dpi = macroquad::miniquad::window::dpi_scale();
    let key = (scale() * 64.0) as u32 * 1000 + (dpi * 64.0) as u32;
    if WARMED.swap(key, Ordering::Relaxed) == key {
        return;
    }
    let font = macroquad::text::get_default_font();
    let chars: Vec<char> = (32u8..127).map(|c| c as char).chain("…–—’→".chars()).collect();
    let caps: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :".chars().collect();
    for size in TEXT_SIZES {
        let px = (raster(size) as f32 * dpi).ceil() as u16;
        font.populate_font_cache(if size > HEADLINE { &caps } else { &chars }, px);
    }
}

fn scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

pub fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba(r, g, b, a)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

pub fn shade(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

fn params(size: f32, color: Color) -> TextParams<'static> {
    let s = scale();
    TextParams {
        font_size: raster(size),
        font_scale: 1.0 / s,
        color,
        ..Default::default()
    }
}

pub fn measure(s: &str, size: f32) -> f32 {
    let sc = scale();
    measure_text(s, None, raster(size), 1.0 / sc)
    .width
}

/// Left-aligned; `y` is the baseline.
pub fn text(s: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_ex(s, x, y, params(size, color));
}

pub fn text_centered(s: &str, cx: f32, y: f32, size: f32, color: Color) {
    text(s, cx - measure(s, size) / 2.0, y, size, color);
}

pub fn text_shadow(s: &str, cx: f32, y: f32, size: f32, color: Color) {
    text_centered(s, cx + 2.0, y + 2.0, size, rgba(0, 0, 0, 180));
    text_centered(s, cx, y, size, color);
}

pub fn wrap(s: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if measure(&candidate, size) > max_w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = candidate;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// A bevelled plate: drop shadow, vertical gradient, top highlight, border.
pub fn panel(r: Rect, fill: Color, border: Color) {
    draw_rectangle(r.x + 3.0, r.y + 4.0, r.w, r.h, rgba(0, 0, 0, 110));
    let steps = 6;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let k = 1.18 - 0.36 * t;
        draw_rectangle(
            r.x,
            r.y + t * r.h,
            r.w,
            r.h / steps as f32 + 0.5,
            shade(fill, k),
        );
    }
    draw_line(
        r.x + 2.0,
        r.y + 2.0,
        r.x + r.w - 2.0,
        r.y + 2.0,
        1.0,
        rgba(255, 255, 255, 30),
    );
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, border);
}

/// A bar background: metal plate with a gradient and a lit edge on `edge_y`.
pub fn bar_background(r: Rect, edge_y: f32) {
    let steps = 8;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let c = Color::new(0.07 + 0.03 * t, 0.075 + 0.03 * t, 0.09 + 0.03 * t, 1.0);
        draw_rectangle(r.x, r.y + t * r.h, r.w, r.h / steps as f32 + 0.5, c);
    }
    // Rivets along the edge.
    let mut x = 10.0;
    while x < r.w {
        let y = if edge_y > r.y + r.h / 2.0 {
            edge_y - 6.0
        } else {
            edge_y + 6.0
        };
        draw_circle(r.x + x, y, 1.6, rgb(70, 66, 58));
        x += 48.0;
    }
    draw_line(r.x, edge_y, r.x + r.w, edge_y, 3.0, rgb(52, 46, 34));
    draw_line(r.x, edge_y, r.x + r.w, edge_y, 1.0, rgb(160, 130, 70));
}

/// Per-frame pointer state. UI widgets consume taps so they don't fall through to the field.
pub struct Ui {
    pub pointer: Vec2,
    pub tap: Option<Vec2>,
    pub consumed: bool,
}

impl Ui {
    pub fn hovered(&self, r: Rect) -> bool {
        r.contains(self.pointer)
    }

    /// Returns true if tapped this frame.
    pub fn hit(&mut self, r: Rect) -> bool {
        match self.tap {
            Some(p) if !self.consumed && r.contains(p) => {
                self.consumed = true;
                true
            }
            _ => false,
        }
    }

    pub fn button(&mut self, r: Rect, lines: &[&str], enabled: bool, accent: Color) -> bool {
        let hover = enabled && self.hovered(r);
        let fill = if !enabled {
            rgb(28, 30, 34)
        } else if hover {
            shade(accent, 0.45)
        } else {
            shade(accent, 0.28)
        };
        panel(r, fill, if enabled { accent } else { rgb(60, 62, 66) });
        let fg = if enabled { WHITE } else { rgb(110, 112, 116) };
        let size = 20.0;
        let total = lines.len() as f32 * size * 1.15;
        for (i, l) in lines.iter().enumerate() {
            let y = r.y + r.h / 2.0 - total / 2.0 + (i as f32 + 0.8) * size * 1.15;
            text_centered(l, r.x + r.w / 2.0, y, size, fg);
        }
        let clicked = self.hit(r);
        clicked && enabled
    }
}
