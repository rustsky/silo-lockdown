//! "Sent to cleaning": a downed rebel leader is put in a cleaning suit, climbs the stair to
//! the airlock, walks out onto the surface, wipes the sensor lens, and collapses in the toxic
//! air. Purely cosmetic; the engine has already removed the leader.

use crate::render::{px, HUD_H, TILE};
use crate::ui::*;
use macroquad::prelude::*;
use silo_core as sc;
use silo_core::map::AIRLOCK;
use std::f32::consts::PI;

/// Surface ground line (top of the bunker's roof slab).
fn ground() -> f32 {
    HUD_H + TILE * 0.8
}
fn door_x() -> f32 {
    (AIRLOCK[0].0 as f32 + 1.0) * TILE
}
/// The surface sensor the cleaners wipe: a camera on a post right of the bunker.
fn lens_x() -> f32 {
    door_x() + 165.0
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    /// Climbing the stair toward the airlock.
    Climb,
    /// Stepping out of the airlock door.
    Out,
    /// Walking across the surface to the sensor.
    Walk,
    /// Scrubbing the lens.
    Wipe,
    /// Staggering away from it.
    Leave,
    Fall,
    Lie,
}

struct Cleaner {
    stage: Stage,
    /// Seconds in the current stage.
    t: f32,
    /// Path distance while climbing.
    dist: f32,
    speed: f32,
    /// Surface x once outside.
    x: f32,
}

pub struct Cleaning {
    list: Vec<Cleaner>,
    /// Grime on the sensor lens (0 clean .. 1 opaque). Builds up; cleaners wipe it off.
    pub dirt: f32,
}

impl Default for Cleaning {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            dirt: 0.7,
        }
    }
}

const OUT_T: f32 = 0.9;
const WALK_SPEED: f32 = 34.0;
const WIPE_T: f32 = 3.2;
const LEAVE_DX: f32 = 46.0;
const FALL_T: f32 = 0.8;
const LIE_T: f32 = 5.0;
const FADE_T: f32 = 1.5;

impl Cleaning {
    /// Send the leader who fell `dist` tiles along the path to cleaning.
    pub fn send(&mut self, map: &sc::Map, dist: f32) {
        let left = (map.length() - dist).max(0.0);
        // Marched up briskly: never longer than ~12 s, however deep they fell.
        let dur = (left / 3.0).clamp(2.0, 12.0);
        self.list.push(Cleaner {
            stage: Stage::Climb,
            t: 0.0,
            dist,
            speed: left / dur,
            x: door_x(),
        });
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Advance by `dt` seconds of game time. Returns where lenses got wiped clean this frame.
    pub fn update(&mut self, map: &sc::Map, dt: f32) -> Vec<Vec2> {
        let mut wiped = Vec::new();
        self.dirt = (self.dirt + dt * 0.004).min(1.0);
        for c in &mut self.list {
            c.t += dt;
            match c.stage {
                Stage::Climb => {
                    c.dist += c.speed * dt;
                    if c.dist >= map.length() {
                        c.stage = Stage::Out;
                        c.t = 0.0;
                    }
                }
                Stage::Out if c.t >= OUT_T => {
                    c.stage = Stage::Walk;
                    c.t = 0.0;
                }
                Stage::Walk => {
                    c.x += WALK_SPEED * dt;
                    if c.x >= lens_x() - 9.0 {
                        c.x = lens_x() - 9.0;
                        c.stage = Stage::Wipe;
                        c.t = 0.0;
                    }
                }
                Stage::Wipe => {
                    self.dirt = (self.dirt - dt / WIPE_T * 1.2).max(0.0);
                    if c.t >= WIPE_T {
                        self.dirt = 0.0;
                        wiped.push(vec2(lens_x(), ground() - 30.0));
                        c.stage = Stage::Leave;
                        c.t = 0.0;
                    }
                }
                Stage::Leave => {
                    // Slowing down: the suit is failing.
                    c.x += WALK_SPEED * 0.55 * (1.0 - c.t / 4.0).max(0.25) * dt;
                    if c.x >= lens_x() - 9.0 + LEAVE_DX {
                        c.stage = Stage::Fall;
                        c.t = 0.0;
                    }
                }
                Stage::Fall if c.t >= FALL_T => {
                    c.stage = Stage::Lie;
                    c.t = 0.0;
                }
                _ => {}
            }
        }
        self.list
            .retain(|c| !(c.stage == Stage::Lie && c.t >= LIE_T + FADE_T));
        wiped
    }

    /// The sensor post on the surface (always there), with its grime.
    pub fn draw_sensor(&self, time: f32) {
        let (x, g) = (lens_x(), ground());
        // Post and base plate.
        draw_rectangle(x - 7.0, g - 3.0, 14.0, 3.0, rgb(70, 70, 66));
        draw_rectangle(x - 1.5, g - 24.0, 3.0, 22.0, rgb(96, 98, 100));
        draw_rectangle(x - 0.5, g - 24.0, 1.0, 22.0, rgb(150, 152, 154));
        // Camera housing with a hood, looking out over the hills.
        draw_rectangle(x - 6.0, g - 33.0, 12.0, 9.0, rgb(118, 120, 116));
        draw_rectangle(x - 7.0, g - 34.5, 14.0, 2.0, rgb(84, 86, 84));
        draw_circle(x, g - 28.5, 3.6, rgb(26, 28, 32));
        draw_circle(x, g - 28.5, 2.4, rgb(60, 90, 110));
        draw_circle(x - 0.9, g - 29.4, 0.9, rgba(220, 240, 255, 200));
        // Grime over the glass.
        if self.dirt > 0.02 {
            let a = (self.dirt * 230.0) as u8;
            draw_circle(x, g - 28.5, 3.7, rgba(88, 76, 58, a));
            draw_circle(x + 1.2, g - 27.6, 1.6, rgba(66, 56, 42, a));
        }
        // Status LED.
        let on = (time * 1.3).fract() < 0.5;
        draw_circle(
            x + 4.2,
            g - 31.6,
            0.9,
            if on { rgb(255, 70, 50) } else { rgb(90, 30, 24) },
        );
    }

    /// Draw every cleaner. Climbing ones are hidden where the cameras can't see.
    pub fn draw(&self, game: &sc::Game, time: f32) {
        let map = game.map();
        for c in &self.list {
            match c.stage {
                Stage::Climb => {
                    let p = map.pos_at(c.dist);
                    if !game.is_visible(p) {
                        continue;
                    }
                    let front = map.is_front(c.dist);
                    let foot = px(p) + vec2(0.0, -4.0);
                    let facing = if front { 1.0 } else { -1.0 };
                    let s = if front { 1.45 } else { 1.25 };
                    let alpha = if front { 1.0 } else { 0.8 };
                    suit(foot, s, facing, c.dist * 9.0, 0.0, None, alpha);
                }
                _ => {
                    let foot = vec2(c.x, ground());
                    let (phase, tilt, hand, alpha) = match c.stage {
                        Stage::Out => (0.0, 0.0, None, (c.t / OUT_T).min(1.0)),
                        Stage::Walk => (c.x * 0.32, 0.0, None, 1.0),
                        Stage::Wipe => {
                            // Small circles over the glass.
                            let a = time * 9.0;
                            let h = vec2(lens_x(), ground() - 28.5)
                                + vec2(a.cos() * 2.2, a.sin() * 1.6);
                            (0.0, 0.0, Some(h), 1.0)
                        }
                        Stage::Leave => {
                            let wobble = (time * 5.0).sin() * 0.12;
                            (c.x * 0.25, wobble, None, 1.0)
                        }
                        Stage::Fall => {
                            let k = (c.t / FALL_T).min(1.0);
                            (0.0, k * k * PI / 2.0, None, 1.0)
                        }
                        _ => {
                            let fade = 1.0 - ((c.t - LIE_T) / FADE_T).clamp(0.0, 1.0);
                            (0.0, PI / 2.0, None, fade)
                        }
                    };
                    suit(foot, 1.45, 1.0, phase, tilt, hand, alpha);
                }
            }
        }
    }
}

/// A figure in a cleaning suit standing on `foot`. `phase` drives the walk cycle, `tilt`
/// rotates the whole body about the feet (falling forward), `hand` reaches for a world point.
fn suit(
    foot: Vec2,
    s: f32,
    facing: f32,
    phase: f32,
    tilt: f32,
    hand: Option<Vec2>,
    alpha: f32,
) {
    let tilt = tilt * facing;
    let (sn, cs) = tilt.sin_cos();
    let w = |x: f32, y: f32| {
        let (x, y) = (x * facing * s, y * s);
        foot + vec2(x * cs - y * sn, x * sn + y * cs)
    };
    let a = |c: Color| with_alpha(c, alpha);
    let line = |p: Vec2, q: Vec2, th: f32, c: Color| draw_line(p.x, p.y, q.x, q.y, th * s, a(c));
    let suit_c = rgb(222, 218, 204);
    let dark = rgb(150, 146, 134);

    // Shadow (only while upright on the ground).
    if tilt.abs() < 0.3 {
        draw_ellipse(foot.x, foot.y, 6.0 * s, 1.4 * s, 0.0, a(rgba(0, 0, 0, 90)));
    }
    // Legs and boots.
    let swing = phase.sin() * 2.2;
    for (k, sw, shade_k) in [(-1.0, -swing, 0.8), (1.0, swing, 1.0)] {
        let f = w(k * 1.0 + sw, 0.0);
        line(w(k * 1.2, -8.0), f, 2.6, shade(suit_c, shade_k));
        line(f, w(k * 1.0 + sw + 1.6, 0.0), 1.8, rgb(40, 40, 42));
    }
    // Air tank on the back.
    let t0 = w(-3.6, -14.5);
    let t1 = w(-3.6, -9.0);
    line(t0, t1, 2.6, rgb(150, 150, 140));
    line(t0, t1, 0.8, rgb(190, 190, 180));
    // Torso.
    let quad = |p: [Vec2; 4], c: Color| {
        draw_triangle(p[0], p[1], p[2], a(c));
        draw_triangle(p[0], p[2], p[3], a(c));
    };
    quad(
        [w(-2.8, -7.5), w(2.8, -7.5), w(3.2, -15.0), w(-3.2, -15.0)],
        suit_c,
    );
    quad(
        [w(-2.8, -7.5), w(-1.0, -7.5), w(-1.4, -15.0), w(-3.2, -15.0)],
        dark,
    );
    // Seal tape around the waist.
    line(w(-2.9, -9.0), w(2.9, -9.0), 1.0, rgb(196, 120, 50));
    // Arm: swinging, or reaching for the lens.
    let sh = w(0.8, -13.8);
    let hnd = hand.unwrap_or_else(|| w(2.0 + (phase + PI).sin() * 1.6, -9.0));
    let elbow = sh.lerp(hnd, 0.5) + vec2(0.0, 1.2 * s);
    line(sh, elbow, 2.0, shade(suit_c, 0.9));
    line(elbow, hnd, 2.0, shade(suit_c, 0.9));
    draw_circle(hnd.x, hnd.y, 1.2 * s, a(rgb(120, 110, 96))); // the wool pad
    // Helmet with a dark gold visor.
    let hc = w(0.3, -17.6);
    draw_circle(hc.x, hc.y, 3.3 * s, a(suit_c));
    let vc = w(1.3, -17.6);
    draw_ellipse(vc.x, vc.y, 2.0 * s, 1.5 * s, tilt, a(rgb(40, 34, 24)));
    let gl = w(1.8, -18.3);
    draw_circle(gl.x, gl.y, 0.6 * s, a(rgb(210, 176, 96)));
}
