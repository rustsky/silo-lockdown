//! Short-lived visual effects driven by engine events.

use crate::light::Lighting;
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

/// Small physics particles: brass casings, sparks, embers, debris.
#[derive(Clone, Copy, PartialEq)]
pub enum PKind {
    Casing,
    Spark,
    Ember,
    Debris,
    /// Soft billowing gas: grows, drifts up a little and thins out.
    Gas,
}

pub struct Particle {
    pos: Vec2,
    vel: Vec2,
    t: f32,
    life: f32,
    kind: PKind,
    color: Color,
    spin: f32,
    /// Starting radius for gas puffs (pixels).
    size: f32,
}

#[derive(Default)]
pub struct Fx {
    list: Vec<Effect>,
    parts: Vec<Particle>,
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

    pub fn particle(&mut self, kind: PKind, pos: Vec2, vel: Vec2, life: f32, color: Color) {
        if self.parts.len() > 1500 {
            return;
        }
        self.parts.push(Particle {
            pos,
            vel,
            t: 0.0,
            life,
            kind,
            color,
            spin: macroquad::rand::gen_range(-12.0, 12.0),
            size: 1.0,
        });
    }

    /// One puff of gas of starting radius `size`.
    pub fn gas(&mut self, pos: Vec2, vel: Vec2, life: f32, size: f32, color: Color) {
        if self.parts.len() > 1500 {
            return;
        }
        self.parts.push(Particle {
            pos,
            vel,
            t: 0.0,
            life,
            kind: PKind::Gas,
            color,
            spin: macroquad::rand::gen_range(0.0, std::f32::consts::TAU),
            size,
        });
    }

    /// A burst of `n` particles flying out of `pos`.
    pub fn burst(&mut self, kind: PKind, pos: Vec2, n: usize, speed: f32, color: Color) {
        use macroquad::rand::gen_range;
        for _ in 0..n {
            let a = gen_range(0.0, std::f32::consts::TAU);
            let v = vec2(a.cos(), a.sin()) * gen_range(0.3, 1.0) * speed;
            let life = gen_range(0.25, 0.7);
            self.particle(kind, pos, v, life, color);
        }
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
        for p in &mut self.parts {
            p.t += dt;
            let gravity = match p.kind {
                PKind::Casing | PKind::Debris => 600.0,
                PKind::Spark => 350.0,
                PKind::Ember => -40.0,
                PKind::Gas => -18.0,
            };
            p.vel.y += gravity * dt;
            let drag = match p.kind {
                PKind::Ember => 1.5,
                PKind::Gas => 2.2,
                _ => 0.6,
            };
            p.vel *= (1.0 - drag * dt).max(0.0);
            p.pos += p.vel * dt;
        }
        self.parts.retain(|p| p.t < p.life);
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
        // Gas first, so sparks and debris read on top of it.
        for p in self.parts.iter().filter(|p| p.kind == PKind::Gas) {
            let k = p.t / p.life;
            // Fade in quickly, then thin out as it spreads.
            let a = (k * 8.0).min(1.0) * (1.0 - k) * (1.0 - k);
            let r = p.size * (1.0 + 1.4 * k);
            let wob = vec2(p.spin.cos(), p.spin.sin()) * r * 0.25;
            let c = p.color;
            draw_circle(p.pos.x, p.pos.y, r, Color::new(c.r, c.g, c.b, c.a * a * 0.45));
            draw_circle(
                p.pos.x + wob.x,
                p.pos.y + wob.y,
                r * 0.62,
                Color::new(
                    (c.r * 1.25).min(1.0),
                    (c.g * 1.15).min(1.0),
                    (c.b * 1.2).min(1.0),
                    c.a * a * 0.38,
                ),
            );
        }
        for p in &self.parts {
            let fade = 1.0 - p.t / p.life;
            match p.kind {
                PKind::Casing => {
                    let a = p.spin * p.t;
                    let d = vec2(a.cos(), a.sin()) * 2.5;
                    draw_line(
                        p.pos.x - d.x,
                        p.pos.y - d.y,
                        p.pos.x + d.x,
                        p.pos.y + d.y,
                        2.0,
                        with_alpha(rgb(220, 180, 80), fade.min(1.0) * 0.9 + 0.1),
                    );
                }
                PKind::Spark => {
                    let tail = p.pos - p.vel * 0.02;
                    draw_line(
                        tail.x,
                        tail.y,
                        p.pos.x,
                        p.pos.y,
                        1.5,
                        with_alpha(p.color, fade),
                    );
                }
                PKind::Ember => draw_circle(p.pos.x, p.pos.y, 1.6, with_alpha(p.color, fade)),
                PKind::Debris => draw_rectangle(
                    p.pos.x - 1.5,
                    p.pos.y - 1.5,
                    3.0,
                    3.0,
                    with_alpha(p.color, fade),
                ),
                PKind::Gas => {}
            }
        }
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

    /// Light emitted by effects (gas clouds, jammer pulses, tracers, flashes).
    pub fn lights(&self, light: &mut Lighting) {
        for p in &self.parts {
            if matches!(p.kind, PKind::Spark | PKind::Ember) {
                let fade = 1.0 - p.t / p.life;
                light.add(p.pos, 8.0, with_alpha(p.color, fade));
            }
        }
        for e in &self.list {
            let fade = 1.0 - e.t / e.life;
            match &e.kind {
                Kind::Ring { max_r, color } => light.add(
                    e.pos,
                    *max_r * 1.2,
                    Color::new(color.r, color.g, color.b, 0.7 * fade),
                ),
                Kind::Cloud { r } => {
                    light.add(e.pos, r * 2.2, Color::new(0.5, 1.0, 0.4, 0.8 * fade));
                }
                Kind::Tracer { to } => {
                    light.add_bloom(*to, 22.0, Color::new(1.0, 0.6, 0.4, fade));
                    light.add_bloom(e.pos, 30.0, Color::new(1.0, 0.8, 0.5, fade));
                }
                _ => {}
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
