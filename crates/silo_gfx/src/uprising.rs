//! How each wave's uprising starts: a rally in front of the main generator at the bottom of
//! the Down Deep. Alarm lights turn red, workers gather and pump their fists while a ringleader
//! on a crate waves the red flag and shouts, then the crowd heads for the stair. The engine
//! holds the first rebel back for `RALLY_TIME`, so the climb starts as the rally ends.
//! Purely cosmetic.

use crate::render::{self, px, HUD_H, TILE};
use crate::ui::*;
use macroquad::prelude::*;
use silo_core as sc;
use silo_core::stats::RALLY_TIME;
use std::f32::consts::PI;

const SLOGANS: &[&str] = &[
    "WE WANT THE TRUTH!",
    "WHAT IS IT HIDING?",
    "THE DEEP RISES!",
    "NO MORE LIES!",
    "WHO BUILT THE SILO?",
    "POWER TO THE DEEP!",
    "OPEN THE RECORDS!",
    "WE KEEP YOU ALIVE!",
    "REMEMBER THE REBELLION!",
    "UP THE STAIRS!",
];

/// When the crowd starts heading for the stair (fraction of the rally).
const LEAVE_AT: f32 = 0.78;

struct Member {
    kind: sc::EnemyKind,
    /// Spot on the hall floor (pixels from the hall's left edge).
    x: f32,
    phase: f32,
}

pub struct Rally {
    t: f32,
    wave: u32,
    orator: sc::EnemyKind,
    crowd: Vec<Member>,
}

#[derive(Default)]
pub struct Uprising {
    rally: Option<Rally>,
    /// Counts down to the next shower of sparks from the generator.
    spark_t: f32,
}

fn hall() -> Rect {
    let tiles = &sc::map::POWER_ROOM;
    let c0 = tiles.iter().map(|t| t.0).min().unwrap() as f32;
    let c1 = tiles.iter().map(|t| t.0).max().unwrap() as f32 + 1.0;
    let r0 = tiles.iter().map(|t| t.1).min().unwrap() as f32;
    let r1 = tiles.iter().map(|t| t.1).max().unwrap() as f32 + 1.0;
    Rect::new(c0 * TILE, HUD_H + r0 * TILE, (c1 - c0) * TILE, (r1 - r0) * TILE - 8.0)
}

impl Uprising {
    /// Wave `n` was just sent: start its rally.
    pub fn start(&mut self, n: u32) {
        let spawns = sc::stats::wave(n);
        let orator = if spawns.iter().any(|s| s.kind == sc::EnemyKind::Leader) {
            sc::EnemyKind::Leader
        } else {
            sc::EnemyKind::Mechanic
        };
        let crowd = spawns
            .iter()
            .filter(|s| s.kind != sc::EnemyKind::Leader)
            .take(6)
            .enumerate()
            .map(|(i, s)| Member {
                kind: s.kind,
                x: 46.0 + i as f32 * 11.0 + if i % 2 == 1 { 3.0 } else { 0.0 },
                phase: i as f32 * 1.3,
            })
            .collect();
        self.rally = Some(Rally { t: 0.0, wave: n, orator, crowd });
        self.spark_t = 0.0;
    }

    pub fn clear(&mut self) {
        self.rally = None;
    }

    /// Advance by `dt` game seconds. Returns points where the generator throws sparks.
    pub fn update(&mut self, dt: f32) -> Vec<Vec2> {
        let mut sparks = Vec::new();
        let Some(r) = &mut self.rally else { return sparks };
        r.t += dt;
        self.spark_t -= dt;
        if self.spark_t <= 0.0 && r.t < RALLY_TIME * LEAVE_AT {
            self.spark_t = 0.35;
            let h = hall();
            let x = h.x + 46.0 + macroquad::rand::gen_range(0.0, 44.0);
            sparks.push(vec2(x, h.y + h.h - 40.0));
        }
        if r.t >= RALLY_TIME + 0.4 {
            self.rally = None;
        }
        sparks
    }

    /// The rally, drawn over the generator hall. Hidden if the cameras can't see down there.
    pub fn draw(&self, game: &sc::Game, time: f32) {
        let Some(r) = &self.rally else { return };
        if !game.is_visible(sc::map::power_center()) {
            return;
        }
        let h = hall();
        let fl = h.y + h.h;
        let k = r.t / RALLY_TIME;

        // Alarm: the hall pulses red, a beacon turns on the ceiling.
        let pulse = 0.5 + 0.5 * (time * 7.0).sin();
        let fade = (1.0 - ((k - 0.9) / 0.2).clamp(0.0, 1.0)) * (r.t / 0.3).min(1.0);
        draw_rectangle(h.x, h.y + 18.0, h.w, h.h - 18.0, Color::new(0.9, 0.05, 0.02, 0.16 * pulse * fade));
        let bc = vec2(h.x + h.w / 2.0, h.y + 22.0);
        draw_rectangle(bc.x - 4.0, bc.y - 3.0, 8.0, 5.0, rgb(60, 20, 18));
        draw_circle(bc.x, bc.y + 2.0, 3.5, Color::new(1.0, 0.15 + 0.3 * pulse, 0.1, fade.max(0.3)));
        let sweep = time * 6.0;
        for side in [-1.0, 1.0] {
            let a = sweep + if side > 0.0 { 0.0 } else { PI };
            let tip = bc + vec2(a.cos() * 60.0, 30.0 + a.sin().abs() * 20.0);
            let n = vec2(-(tip - bc).y, (tip - bc).x).normalize() * 10.0;
            draw_triangle(bc, tip + n, tip - n, Color::new(1.0, 0.1, 0.05, 0.12 * fade * a.cos().abs()));
        }

        // The crowd faces the orator, fists in the air; then turns for the stair.
        let leaving = ((k - LEAVE_AT) / (1.0 - LEAVE_AT)).clamp(0.0, 1.0);
        let gather = (r.t / 0.5).min(1.0);
        let stair_x = px(sc::map::waypoints()[0]).x + 6.0;
        for (i, m) in r.crowd.iter().enumerate() {
            // Each member leaves in turn and walks off toward the foot of the stair.
            let my_leave = ((leaving * (r.crowd.len() as f32 + 2.0)) - i as f32).clamp(0.0, 1.0);
            let x0 = h.x + m.x + (1.0 - gather) * 30.0;
            let x = x0 + (stair_x - x0) * my_leave;
            if my_leave >= 1.0 {
                continue;
            }
            let walking = gather < 1.0 || my_leave > 0.0;
            let mut e = sc::Enemy::for_display(m.kind, to_tiles(vec2(x, fl)));
            e.blocked = !walking;
            e.id = i as u32 + 1;
            // Facing the orator (left) while chanting; facing the stair (right) once leaving.
            let toward_stair = my_leave > 0.0 || gather < 1.0;
            render::rebel(&e, toward_stair, time);
            if !walking {
                fist(vec2(x, fl - 4.0), time * 6.5 + m.phase, if toward_stair { 1.0 } else { -1.0 });
            }
        }

        // The orator on a crate, flag high.
        let ox = h.x + 26.0;
        let crate_r = Rect::new(ox - 9.0, fl - 11.0, 18.0, 11.0);
        draw_rectangle(crate_r.x, crate_r.y, crate_r.w, crate_r.h, rgb(120, 88, 52));
        draw_rectangle_lines(crate_r.x, crate_r.y, crate_r.w, crate_r.h, 1.2, rgb(70, 50, 28));
        draw_line(crate_r.x, crate_r.y, crate_r.x + crate_r.w, crate_r.y + crate_r.h, 1.0, rgb(84, 62, 36));
        // The orator follows the crowd out last, flag and all.
        let n = r.crowd.len() as f32;
        let o_leave = (leaving * (n + 2.0) - n).clamp(0.0, 1.0);
        if o_leave < 1.0 {
            let foot = if o_leave > 0.0 {
                vec2(ox + (stair_x - ox) * o_leave, fl)
            } else {
                vec2(ox, crate_r.y)
            };
            let mut o = sc::Enemy::for_display(r.orator, to_tiles(foot));
            o.id = 99;
            o.blocked = o_leave == 0.0;
            if r.orator != sc::EnemyKind::Leader {
                flag(foot + vec2(5.0, -18.0), time);
            }
            render::rebel(&o, true, time);
            if o_leave == 0.0 {
                fist(foot, time * 4.0, 1.0);
            }
            // The shout.
            if o_leave == 0.0 && (0.12..LEAVE_AT + 0.05).contains(&k) {
                let slogan = SLOGANS[(r.wave as usize - 1) % SLOGANS.len()];
                let size = 14.0;
                let w = measure(slogan, size) + 12.0;
                let bx = (ox - 10.0).max(4.0);
                let by = foot.y - 64.0;
                draw_rectangle(bx, by - 15.0, w, 20.0, rgba(250, 244, 230, 235));
                draw_triangle(
                    vec2(bx + 10.0, by + 5.0),
                    vec2(bx + 18.0, by + 5.0),
                    vec2(ox + 2.0, foot.y - 46.0),
                    rgba(250, 244, 230, 235),
                );
                text(slogan, bx + 6.0, by, size, rgb(150, 20, 16));
            }
        }
    }
}

/// World pixel (rebel feet) -> engine coordinates for `render::rebel`, which stands figures 4 px
/// above `px(pos)`.
fn to_tiles(foot: Vec2) -> sc::Vec2 {
    sc::vec2(foot.x / TILE, (foot.y + 4.0 - HUD_H) / TILE)
}

/// A raised arm pumping a fist above a figure standing at `foot`.
fn fist(foot: Vec2, phase: f32, facing: f32) {
    let pump = phase.sin().max(-0.3);
    let sh = foot + vec2(2.0 * facing, -24.0);
    let hand = sh + vec2(3.0 * facing, -9.0 - 4.0 * pump);
    draw_line(sh.x, sh.y, hand.x, hand.y, 2.6, rgb(120, 92, 70));
    draw_circle(hand.x, hand.y, 2.2, rgb(214, 172, 140));
}

/// A red flag on a pole whose foot is at `base`, waving.
fn flag(base: Vec2, time: f32) {
    let top = base + vec2(0.0, -30.0);
    draw_line(base.x, base.y, top.x, top.y, 1.8, rgb(90, 70, 50));
    let seg = 5;
    for i in 0..seg {
        let x0 = i as f32 * 4.0;
        let x1 = x0 + 4.0;
        let w0 = (time * 6.0 - i as f32 * 0.9).sin() * 2.0;
        let w1 = (time * 6.0 - (i + 1) as f32 * 0.9).sin() * 2.0;
        let a = top + vec2(x0, w0);
        let b = top + vec2(x1, w1);
        let c = b + vec2(0.0, 11.0);
        let d = a + vec2(0.0, 11.0);
        let col = if i % 2 == 0 { rgb(190, 30, 26) } else { rgb(170, 24, 22) };
        draw_triangle(a, b, c, col);
        draw_triangle(a, c, d, col);
    }
}
