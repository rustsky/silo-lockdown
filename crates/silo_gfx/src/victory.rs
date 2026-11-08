//! Victory: the last wave is held. An "all clear" sweeps up the silo, the phone in IT's server
//! hall rings, and Silo 1 comes on a secure video line to congratulate the Head of IT, with
//! subtitles typing out as it speaks. Purely cosmetic; a tap skips it.

use crate::render::{HUD_H, PANEL_Y, TILE, W, WORLD_H};
use crate::ui::*;
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

/// The all-clear sweeps from the generator hall to the airlock.
const CLEAR_T: f32 = 3.0;
/// The phone rings twice.
const RINGS: [f32; 2] = [3.3, 4.9];
/// The video line opens.
const CALL_AT: f32 = 6.5;
/// Lengths of Silo 1's recorded lines (voice_4.mp3, voice_5.mp3), in seconds.
const LINE_0_SECS: f32 = 9.25;
const LINE_1_SECS: f32 = 8.59;
/// Silo 1's two lines: (start after the call opens, length of the recording, text).
const LINES: [(f32, f32, &str); 2] = [
    (0.3, LINE_0_SECS, "This is Silo One, calling Silo Twelve. Head of I.T., the uprising in your silo has been put down."),
    (
        LINE_0_SECS + 0.5,
        LINE_1_SECS,
        "Your silo held, and the Pact is preserved. Well done. The Safeguard will not be needed. Silo One out.",
    ),
];
/// After the last line: "TRANSMISSION ENDED", then the line closes.
const END_AT: f32 = CALL_AT + LINES[1].0 + LINES[1].1 + 0.4;
const DONE_AT: f32 = END_AT + 1.8;

/// What the app should add this frame.
#[derive(Default)]
pub struct Cues {
    pub ring: bool,
    /// The video line opens (a burst of static).
    pub line_open: bool,
    /// Start Silo 1's line `n`.
    pub line: Option<usize>,
}

#[derive(Default)]
pub struct Victory {
    t: f32,
    on: bool,
    /// Loudness of Silo 1's two lines, 60 values per second (drives the waveform).
    envelopes: Vec<Vec<f32>>,
}

/// IT's server hall in world pixels.
fn it_rect() -> Rect {
    let tiles = &silo_core::map::IT_ROOM;
    let c0 = tiles.iter().map(|t| t.0).min().unwrap() as f32;
    let c1 = tiles.iter().map(|t| t.0).max().unwrap() as f32 + 1.0;
    let r0 = tiles.iter().map(|t| t.1).min().unwrap() as f32;
    let r1 = tiles.iter().map(|t| t.1).max().unwrap() as f32 + 1.0;
    Rect::new(c0 * TILE, HUD_H + r0 * TILE, (c1 - c0) * TILE, (r1 - r0) * TILE - 8.0)
}

fn crossed(prev: f32, now: f32, at: f32) -> bool {
    prev < at && now >= at
}

impl Victory {
    pub fn start(&mut self) {
        let envelopes = vec![
            crate::sfx::voice_envelope(crate::sfx::VOICE_SILO1_HELLO),
            crate::sfx::voice_envelope(crate::sfx::VOICE_SILO1_WELL_DONE),
        ];
        *self = Victory { t: 0.0, on: true, envelopes };
    }

    /// How loud Silo 1 is right now (0..1), `u` seconds after the call opened.
    fn loudness(&self, u: f32) -> f32 {
        for (i, &(s, d, _)) in LINES.iter().enumerate() {
            if (s..s + d).contains(&u) {
                let env = &self.envelopes[i];
                // The clip opens with the line-open beep (0.37 s) before the voice: skip it.
                let k = ((u - s) * 60.0) as usize;
                return env.get(k).copied().unwrap_or(0.0);
            }
        }
        0.0
    }

    pub fn clear(&mut self) {
        *self = Victory::default();
    }

    pub fn active(&self) -> bool {
        self.on && self.t < DONE_AT
    }

    pub fn skip(&mut self) {
        if self.on {
            self.t = DONE_AT;
        }
    }

    /// Silo 1 is speaking (the music sits lower).
    pub fn on_the_line(&self) -> bool {
        self.active() && self.t >= CALL_AT && self.t < END_AT
    }

    /// The sequence drives the camera: rise with the all-clear, then stay at the top.
    pub fn camera(&self, max_scroll: f32) -> Option<f32> {
        if !self.active() {
            return None;
        }
        let k = (self.t / CLEAR_T).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        Some((1.0 - k) * max_scroll)
    }

    pub fn update(&mut self, dt: f32) -> Cues {
        let mut cues = Cues::default();
        if !self.active() {
            return cues;
        }
        let (prev, now) = (self.t, self.t + dt);
        self.t = now;
        cues.ring = RINGS.iter().any(|&r| crossed(prev, now, r));
        cues.line_open = crossed(prev, now, CALL_AT);
        for (i, (start, _, _)) in LINES.iter().enumerate() {
            if crossed(prev, now, CALL_AT + start) {
                cues.line = Some(i);
            }
        }
        cues
    }

    /// In the world: the all-clear sweep, and the ringing phone in IT's hall.
    pub fn draw_world(&self, time: f32) {
        if !self.active() {
            return;
        }
        let top = HUD_H + TILE;
        let bottom = HUD_H + WORLD_H;
        // A band of green light rising up the silo; what it has passed glows faintly.
        let k = (self.t / CLEAR_T).clamp(0.0, 1.0);
        let front = bottom - k * (bottom - top);
        let after = (1.0 - ((self.t - CLEAR_T) / 2.0).clamp(0.0, 1.0)) * 0.08;
        draw_rectangle(0.0, front, W, bottom - front, Color::new(0.3, 1.0, 0.5, after));
        if k < 1.0 {
            for i in 0..12 {
                let y = front + i as f32 * 3.0;
                draw_rectangle(0.0, y, W, 3.0, Color::new(0.4, 1.0, 0.55, 0.32 * (1.0 - i as f32 / 12.0)));
            }
            text_centered("ALL CLEAR", W / 2.0, front - 8.0, 24.0, rgb(150, 255, 170));
        }
        // The phone in the server hall, flashing and ringing.
        if (RINGS[0]..CALL_AT).contains(&self.t) {
            let r = it_rect();
            let p = vec2(r.x + r.w - 22.0, r.y + 34.0);
            let flash = (time * 8.0).sin() > 0.0;
            draw_rectangle(p.x - 9.0, p.y - 3.0, 18.0, 9.0, rgb(30, 30, 34));
            draw_rectangle(p.x - 11.0, p.y - 8.0, 22.0, 5.0, rgb(50, 50, 56));
            draw_circle(p.x + 6.0, p.y + 1.0, 2.0, if flash { rgb(255, 50, 40) } else { rgb(90, 20, 18) });
            let ringing = RINGS.iter().any(|&s| (s..s + 1.05).contains(&self.t));
            if ringing {
                for i in 1..=3 {
                    let rr = 8.0 + i as f32 * 5.0 + (time * 12.0).sin() * 1.5;
                    for side in [-1.0f32, 1.0] {
                        let a0 = if side < 0.0 { PI * 0.8 } else { -PI * 0.2 };
                        let pts: Vec<Vec2> = (0..=6)
                            .map(|j| {
                                let a = a0 + j as f32 / 6.0 * PI * 0.4;
                                p + vec2(a.cos(), a.sin()) * rr
                            })
                            .collect();
                        for w in pts.windows(2) {
                            draw_line(w[0].x, w[0].y, w[1].x, w[1].y, 1.5, rgba(255, 220, 120, 200));
                        }
                    }
                }
            }
            let label = "INCOMING: SILO 1";
            let w = measure(label, 12.0) + 12.0;
            let lx = (p.x - w / 2.0).max(4.0);
            draw_rectangle(lx, p.y - 34.0, w, 16.0, rgba(60, 10, 10, 230));
            text(label, lx + 6.0, p.y - 22.0, 12.0, if flash { rgb(255, 210, 200) } else { rgb(220, 140, 130) });
        }
    }

    /// On screen: Silo 1 on the line. No face: only its voice, as a waveform that moves with
    /// every word, in a cold red, with the identity withheld.
    pub fn draw_screen(&self, time: f32) {
        if !self.active() || self.t < CALL_AT {
            return;
        }
        let u = self.t - CALL_AT;
        // Opens and closes like an old CRT.
        let open = (u / 0.35).min(1.0) * (1.0 - ((self.t - END_AT - 1.2) / 0.4).clamp(0.0, 1.0));
        if open <= 0.0 {
            return;
        }
        let red = rgb(255, 52, 36);
        let dim = rgb(120, 22, 18);
        let full = Rect::new(24.0, HUD_H + 70.0, W - 48.0, 380.0);
        let h = full.h * open;
        let r = Rect::new(full.x, full.y + (full.h - h) / 2.0, full.w, h);
        draw_rectangle(0.0, HUD_H, W, PANEL_Y - HUD_H, Color::new(0.0, 0.0, 0.0, 0.6 * open));
        draw_rectangle(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, dim);
        draw_rectangle(r.x, r.y, r.w, r.h, rgb(6, 3, 3));
        if open < 1.0 {
            for _ in 0..60 {
                let x = r.x + macroquad::rand::gen_range(0.0, r.w);
                let y = r.y + macroquad::rand::gen_range(0.0, r.h.max(1.0));
                let g = macroquad::rand::gen_range(0.3, 0.9);
                draw_rectangle(x, y, macroquad::rand::gen_range(4.0, 30.0), 2.0, Color::new(g, g * 0.3, g * 0.25, 0.6));
            }
            return;
        }
        // Speech peaks hit hard: lift the quiet parts so the trace never looks timid.
        let loud = self.loudness(u).powf(0.55);
        // Header: who is calling, and nothing else.
        draw_rectangle(r.x, r.y, r.w, 30.0, rgb(28, 6, 5));
        text("SILO 1", r.x + 12.0, r.y + 22.0, 20.0, red);
        text("IDENTITY WITHHELD", r.x + 92.0, r.y + 21.0, 14.0, rgb(150, 60, 50));
        if (time * 1.5).fract() < 0.6 {
            draw_circle(r.x + r.w - 58.0, r.y + 15.0, 5.0, red);
        }
        text("LIVE", r.x + r.w - 48.0, r.y + 21.0, 16.0, rgb(255, 120, 100));

        // The voice. A faint grid, then the trace: a line that only moves when Silo 1 speaks.
        let wave = Rect::new(r.x + 14.0, r.y + 44.0, r.w - 28.0, 150.0);
        let cy = wave.y + wave.h / 2.0;
        let mut gx = wave.x;
        while gx <= wave.x + wave.w {
            draw_line(gx, wave.y, gx, wave.y + wave.h, 1.0, rgba(90, 20, 16, 70));
            gx += wave.w / 12.0;
        }
        for k in 0..5 {
            let y = wave.y + k as f32 * wave.h / 4.0;
            draw_line(wave.x, y, wave.x + wave.w, y, 1.0, rgba(90, 20, 16, if k == 2 { 140 } else { 70 }));
        }
        let n = 160;
        let pts: Vec<Vec2> = (0..=n)
            .map(|i| {
                let f = i as f32 / n as f32;
                let x = wave.x + f * wave.w;
                // Louder in the middle, quiet at the edges.
                let win = (f * std::f32::consts::PI).sin().powf(1.5);
                let carrier = (f * 31.0 - time * 11.0).sin() * 0.5
                    + (f * 67.0 + time * 17.0).sin() * 0.3
                    + (f * 13.0 - time * 5.0).sin() * 0.2;
                // Jagged teeth on top: a harsh, inhuman edge.
                let teeth = ((f * 211.0 + time * 63.0).sin() * (f * 157.0 - time * 41.0).sin()).signum()
                    * ((f * 401.0 + time * 90.0).sin()).abs()
                    * 0.35;
                let hum = (f * 90.0 + time * 40.0).sin() * 0.6; // idle hum on the line
                let amp = ((carrier + teeth * loud) * loud * wave.h * 0.62).clamp(-wave.h * 0.48, wave.h * 0.48);
                let y = cy - (amp + hum) * win;
                vec2(x, y)
            })
            .collect();
        // Glow, then the bright trace, then a faint mirror.
        for (w, a) in [(9.0, 0.10), (5.0, 0.22)] {
            for p in pts.windows(2) {
                draw_line(p[0].x, p[0].y, p[1].x, p[1].y, w, Color::new(1.0, 0.18, 0.12, a * (0.4 + loud)));
            }
        }
        for p in pts.windows(2) {
            draw_line(p[0].x, p[0].y, p[1].x, p[1].y, 2.0, Color::new(1.0, 0.45 + 0.4 * loud, 0.35, 1.0));
            draw_line(p[0].x, 2.0 * cy - p[0].y, p[1].x, 2.0 * cy - p[1].y, 1.0, rgba(200, 40, 30, 90));
        }
        // The whole screen throbs red on loud syllables.
        draw_rectangle(r.x, r.y + 30.0, r.w, r.h - 30.0, Color::new(0.5, 0.02, 0.0, 0.12 * loud));
        // Glitch: now and then a slice of the trace jumps sideways.
        let g = (time * 3.7).sin() * (time * 11.3).cos();
        if g > 0.82 || loud > 0.92 {
            let gy = wave.y + ((time * 97.0).sin() * 0.5 + 0.5) * wave.h;
            let off = (time * 53.0).sin() * 18.0;
            draw_rectangle(wave.x + off, gy, wave.w, 5.0, rgba(255, 60, 40, 60));
        }

        // Subtitles, typed as Silo 1 speaks.
        let tx = r.x + 18.0;
        let tw = r.w - 36.0;
        let mut y = wave.y + wave.h + 34.0;
        for &(s, d, line) in LINES.iter() {
            if u < s {
                break;
            }
            let shown = (((u - s) / (d * 0.85)).min(1.0) * line.chars().count() as f32) as usize;
            // Only the current line; the earlier one is gone from the screen.
            if u > s + d + 0.3 && LINES.iter().any(|&(s2, _, _)| s2 > s && u >= s2) {
                continue;
            }
            let mut left = shown;
            for l in wrap(line, 20.0, tw) {
                let k = l.chars().count();
                let part: String = l.chars().take(left.min(k)).collect();
                text(&part, tx, y, 20.0, rgb(240, 214, 208));
                left = left.saturating_sub(k + 1);
                y += 26.0;
            }
        }
        if self.t >= END_AT && (time * 2.0).fract() < 0.7 {
            text_centered("TRANSMISSION ENDED", r.x + r.w / 2.0, r.y + r.h - 18.0, 20.0, rgb(255, 120, 90));
        }
        // CRT scanlines and a slow roll bar.
        let mut sy = r.y;
        while sy < r.y + r.h {
            draw_line(r.x, sy, r.x + r.w, sy, 1.0, rgba(0, 0, 0, 60));
            sy += 3.0;
        }
        let roll = r.y + (time * 40.0).rem_euclid(r.h);
        draw_rectangle(r.x, roll, r.w, 14.0, Color::new(1.0, 0.3, 0.2, 0.03 + 0.02 * (time * TAU).sin()));
    }
}
