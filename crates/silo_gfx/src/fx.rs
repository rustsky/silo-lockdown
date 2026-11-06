//! Short-lived visual effects driven by engine events.

use crate::ui::*;
use macroquad::prelude::*;

pub enum Kind {
    Ring { max_r: f32, color: Color },
    Cloud { r: f32 },
    Tracer { to: Vec2 },
    Text { text: String, color: Color },
    Puff,
}

pub struct Effect {
    pos: Vec2,
    t: f32,
    life: f32,
    kind: Kind,
}

#[derive(Default)]
pub struct Fx {
    list: Vec<Effect>,
    pub shake: f32,
    pub red_flash: f32,
    toast: Option<(String, f32)>,
}

impl Fx {
    pub fn add(&mut self, pos: Vec2, life: f32, kind: Kind) {
        self.list.push(Effect {
            pos,
            t: 0.0,
            life,
            kind,
        });
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), 1.8));
    }

    pub fn clear(&mut self) {
        *self = Fx::default();
    }

    pub fn update(&mut self, dt: f32) {
        for e in &mut self.list {
            e.t += dt;
        }
        self.list.retain(|e| e.t < e.life);
        self.shake = (self.shake - dt).max(0.0);
        self.red_flash = (self.red_flash - dt).max(0.0);
        if let Some((_, t)) = &mut self.toast {
            *t -= dt;
            if *t <= 0.0 {
                self.toast = None;
            }
        }
    }

    pub fn draw(&self) {
        for e in &self.list {
            let k = e.t / e.life; // 0..1
            let fade = 1.0 - k;
            match &e.kind {
                Kind::Ring { max_r, color } => {
                    draw_circle_lines(
                        e.pos.x,
                        e.pos.y,
                        max_r * (0.3 + 0.7 * k),
                        3.0,
                        with_alpha(*color, fade),
                    );
                }
                Kind::Cloud { r } => {
                    for i in 0..6 {
                        let a = i as f32 * 1.047 + e.t * 0.8;
                        let rr = r * (0.45 + 0.5 * k);
                        let p = e.pos + vec2(a.cos(), a.sin()) * rr * 0.45;
                        draw_circle(p.x, p.y, rr * 0.6, rgba(120, 210, 90, (fade * 90.0) as u8));
                    }
                    draw_circle(
                        e.pos.x,
                        e.pos.y,
                        r * 0.5 * (1.0 - k * 0.5),
                        rgba(200, 255, 150, (fade * 120.0) as u8),
                    );
                }
                Kind::Tracer { to } => {
                    draw_line(
                        e.pos.x,
                        e.pos.y,
                        to.x,
                        to.y,
                        3.0 * fade + 0.5,
                        rgba(255, 120, 100, (fade * 255.0) as u8),
                    );
                    draw_circle(
                        to.x,
                        to.y,
                        6.0 * fade,
                        rgba(255, 220, 160, (fade * 255.0) as u8),
                    );
                }
                Kind::Text { text: s, color } => {
                    let y = e.pos.y - 28.0 * k;
                    text_centered(
                        s,
                        e.pos.x + 1.0,
                        y + 1.0,
                        18.0,
                        rgba(0, 0, 0, (fade * 200.0) as u8),
                    );
                    text_centered(s, e.pos.x, y, 18.0, with_alpha(*color, fade));
                }
                Kind::Puff => {
                    for i in 0..5 {
                        let a = i as f32 * 1.256;
                        let p = e.pos + vec2(a.cos(), a.sin() - 0.6) * 14.0 * k;
                        draw_circle(
                            p.x,
                            p.y,
                            4.0 * fade + 1.0,
                            rgba(170, 160, 150, (fade * 160.0) as u8),
                        );
                    }
                }
            }
        }
    }

    pub fn draw_toast(&self, cx: f32, y: f32) {
        if let Some((s, t)) = &self.toast {
            let a = t.min(1.0);
            let w = measure(s, 18.0) + 28.0;
            draw_rectangle(
                cx - w / 2.0,
                y - 22.0,
                w,
                32.0,
                rgba(60, 10, 10, (a * 220.0) as u8),
            );
            text_centered(s, cx, y, 18.0, rgba(255, 190, 180, (a * 255.0) as u8));
        }
    }
}
