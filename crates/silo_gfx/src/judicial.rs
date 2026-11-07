//! Judicial: Sims' agents arrest rebels the player pays for. An agent in a black suit steps
//! in beside the rebel and cuffs them; every arrested rebel, flag or not, is then sent to
//! cleaning, escorted up to the airlock by the agent (see `cleaning`). Purely cosmetic: the
//! engine has already removed the rebel.

use crate::render::{self, px};
use crate::ui::*;
use macroquad::prelude::*;
use silo_core as sc;

/// Agent steps in.
const IN_T: f32 = 0.35;
/// Cuffing; then they are handed over for cleaning.
const HOLD_T: f32 = 1.1;

struct Arrest {
    rebel: sc::Enemy,
    t: f32,
}

#[derive(Default)]
pub struct Judicial {
    list: Vec<Arrest>,
}

impl Judicial {
    /// Start the arrest scene for `rebel` (a copy taken just before the engine removed it).
    pub fn arrest(&mut self, mut rebel: sc::Enemy) {
        rebel.blocked = true; // stands still while cuffed
        self.list.push(Arrest { rebel, t: 0.0 });
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Advance by `dt` game seconds. Returns where along the path each rebel whose cuffing just
    /// finished stands: they go to cleaning from there.
    pub fn update(&mut self, dt: f32) -> Vec<f32> {
        let mut done = Vec::new();
        for a in &mut self.list {
            a.t += dt;
            if a.t >= IN_T + HOLD_T {
                done.push(a.rebel.dist);
            }
        }
        self.list.retain(|a| a.t < IN_T + HOLD_T);
        done
    }

    pub fn draw(&self, game: &sc::Game, time: f32) {
        let map = game.map();
        for a in &self.list {
            let e = &a.rebel;
            if !game.is_visible(e.pos) {
                continue;
            }
            let front = map.is_front(e.dist);
            // Heading back down: the opposite way the rebel was climbing.
            let away = if front { -1.0 } else { 1.0 };
            let foot = px(e.pos) + vec2(e.lane * render::TILE * 0.9, -4.0);
            let fade_in = (a.t / IN_T).min(1.0);
            let s = if front { 1.5 } else { 1.3 };
            let agent_foot = foot + vec2(away * 13.0 * s / 1.5, 0.0);
            let phase = 0.0;
            render::rebel(e, front, time);
            // Cuffs glinting at the rebel's wrists once cuffed.
            if a.t > IN_T + 0.25 {
                let w = foot + vec2(0.0, -16.0 * s / 1.5);
                let glint = 0.6 + 0.4 * (time * 8.0).sin();
                draw_circle_lines(w.x - 2.0, w.y, 2.0, 1.2, rgba(210, 214, 220, (glint * 255.0) as u8));
                draw_circle_lines(w.x + 2.0, w.y, 2.0, 1.2, rgba(210, 214, 220, (glint * 255.0) as u8));
            }
            agent(agent_foot, s, -away, phase, fade_in, foot);
        }
    }
}

/// An agent walking behind a prisoner: `grab` is the prisoner's feet.
pub fn escort(foot: Vec2, s: f32, facing: f32, phase: f32, alpha: f32, grab: Vec2) {
    agent(foot, s, facing, phase, alpha, grab);
}

/// An agent standing on `foot`, arm out, for the panel card.
pub fn agent_icon(foot: Vec2, s: f32, alpha: f32) {
    agent(foot, s, 1.0, 0.0, alpha, foot + vec2(13.0 * s / 1.5, 6.0 * s / 1.5));
}

/// One of Sims' Judicial agents: black suit, white shirt, dark tie, a gold badge. Faces
/// `facing` (toward the rebel), one hand on the rebel's shoulder at `grab`.
fn agent(foot: Vec2, s: f32, facing: f32, phase: f32, alpha: f32, grab: Vec2) {
    let a = |c: Color| with_alpha(c, alpha);
    let k = s / 1.5;
    let w = |x: f32, y: f32| foot + vec2(x * facing * k, y * k);
    let line = |p: Vec2, q: Vec2, th: f32, c: Color| draw_line(p.x, p.y, q.x, q.y, th * k, a(c));
    // Charcoal rather than pure black so the agent reads against the dark stairwell.
    let suit = rgb(48, 50, 62);
    let suit_hi = rgb(92, 96, 116);

    draw_ellipse(foot.x, foot.y + 1.0, 9.0 * k, 2.4 * k, 0.0, a(rgba(0, 0, 0, 110)));
    // Legs.
    let sw = phase.sin() * 3.0;
    for (dx, sh) in [(-2.4, 0.8), (2.4, 1.0)] {
        let swing = if dx < 0.0 { -sw } else { sw };
        line(w(dx * 0.6, -13.0), w(dx + swing, -1.5), 3.4, shade(suit, sh));
        line(w(dx + swing - 0.5, -1.0), w(dx + swing + 3.0, -1.0), 2.4, rgb(10, 10, 12));
    }
    // Jacket.
    let (hl, hr, sl, sr) = (w(-5.0, -12.0), w(5.0, -12.0), w(-6.5, -26.0), w(6.5, -26.0));
    draw_triangle(hl, hr, sr, a(suit));
    draw_triangle(hl, sr, sl, a(suit));
    draw_triangle(hl, w(-1.0, -12.0), sl, a(suit_hi));
    // Rim light along the back and shoulders.
    line(sl, hl, 1.2, rgb(150, 156, 176));
    line(sl, w(-1.0, -27.0), 1.2, rgb(150, 156, 176));
    // Shirt, tie, badge.
    draw_triangle(w(-2.2, -26.0), w(2.2, -26.0), w(0.0, -20.0), a(rgb(230, 230, 226)));
    line(w(0.0, -25.0), w(0.0, -19.0), 1.4, rgb(70, 16, 18));
    draw_poly(w(3.2, -22.0).x, w(3.2, -22.0).y, 5, 1.6 * k, 0.0, a(rgb(240, 200, 80)));
    // Arm reaching to the rebel's shoulder.
    let shoulder = w(4.5, -24.0);
    let hand = grab + vec2(0.0, -26.0 * k);
    let elbow = shoulder.lerp(hand, 0.5) + vec2(0.0, 3.0 * k);
    line(shoulder, elbow, 3.0, suit);
    line(elbow, hand, 3.0, suit);
    draw_circle(hand.x, hand.y, 1.7 * k, a(rgb(214, 172, 140)));
    // Head: face, slicked dark hair.
    let head = w(0.5, -30.0);
    draw_circle(head.x, head.y, 4.0 * k, a(rgb(214, 172, 140)));
    draw_circle(head.x - 1.2 * facing * k, head.y - 1.8 * k, 3.6 * k, a(rgb(28, 22, 18)));
    let eye = w(2.2, -30.2);
    draw_circle(eye.x, eye.y, 0.7 * k, a(rgb(20, 20, 20)));
}
