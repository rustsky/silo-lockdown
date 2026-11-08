//! Defeat: the uprising breaks out through the airlock and Silo 1 answers with the Safeguard
//! protocol. Rebels pour out onto the surface and cheer; drones arrive and shoot every one of
//! them; then gas floods the silo from the top down while the view sinks with it; finally an
//! end card. Purely cosmetic, played after the engine has declared the game lost. A tap skips it.

use crate::render::{self, HUD_H, PANEL_Y, TILE, W, WORLD_H};
use crate::ui::*;
use macroquad::prelude::*;
use silo_core as sc;
use std::f32::consts::PI;

/// Rebels file out of the airlock and spread across the surface.
const SAFEGUARD_AT: f32 = 3.2;
/// The drones fly in.
const DRONES_AT: f32 = 3.9;
/// The gas never starts before Silo 1's first announcement (9.3 s from the alarm) is over...
const GAS_EARLIEST: f32 = SAFEGUARD_AT + 8.9;
/// ...and starts by this time even if a drone is still busy.
const GAS_LATEST: f32 = 14.0;
/// Gas takes this long to sink from the top of the silo to the generator hall.
const GAS_T: f32 = 6.0;
/// The end card shows this long before the game-over panel (the "game over" line fits).
const CARD_T: f32 = 6.4;

const DRONE_Y: f32 = HUD_H + 14.0;
const DRONE_SPEED: f32 = 150.0;

fn ground() -> f32 {
    HUD_H + TILE * 0.8
}
fn door_x() -> f32 {
    (sc::map::AIRLOCK[0].0 as f32 + 1.0) * TILE
}

struct Escapee {
    kind: sc::EnemyKind,
    x: f32,
    target: f32,
    /// When they come out of the door.
    out_at: f32,
    /// When a drone shot them, if it has.
    shot_at: Option<f32>,
    flag: bool,
}

struct Drone {
    pos: Vec2,
    target: Option<usize>,
    cooldown: f32,
}

/// What the app should add on top this frame.
#[derive(Default)]
pub struct Cues {
    /// The first rebels are out: the crowd cheers.
    pub freed: bool,
    /// "SAFEGUARD PROTOCOL INITIATED": banner, alarms, announcement.
    pub alarm: bool,
    /// The end card: "Game over" announcement.
    pub game_over: bool,
    /// Drone shots: (drone, target).
    pub shots: Vec<(Vec2, Vec2)>,
    /// The gas starts flooding the silo.
    pub gas_start: bool,
    /// Gas puffs to emit (vents near the gas front).
    pub puffs: Vec<Vec2>,
}

#[derive(Default)]
pub struct Safeguard {
    t: f32,
    on: bool,
    escapees: Vec<Escapee>,
    drones: Vec<Drone>,
    /// When the gas started, once it has.
    gas_at: Option<f32>,
    puff_t: f32,
    alarmed: bool,
    cheered: bool,
    announced_end: bool,
}

impl Safeguard {
    /// The silo has fallen: start the sequence with `n` rebels breaking out.
    pub fn start(&mut self, n: usize) {
        use macroquad::rand::gen_range;
        let kinds = [sc::EnemyKind::Mechanic, sc::EnemyKind::Porter, sc::EnemyKind::Miner];
        let escapees = (0..n)
            .map(|i| {
                // Spread across open ground on both sides of the bunker.
                let target = if i % 2 == 0 {
                    gen_range(470.0, W - 80.0)
                } else {
                    gen_range(30.0, door_x() - 120.0)
                };
                Escapee {
                    kind: kinds[i % kinds.len()],
                    x: door_x(),
                    target,
                    out_at: i as f32 * 0.22,
                    shot_at: None,
                    flag: i == 0,
                }
            })
            .collect();
        *self = Safeguard { on: true, escapees, ..Default::default() };
    }

    pub fn clear(&mut self) {
        *self = Safeguard::default();
    }

    /// Playing (the game-over panel waits until it's done).
    pub fn active(&self) -> bool {
        self.on && !self.done()
    }

    pub fn done(&self) -> bool {
        self.gas_at.is_some_and(|g| self.t >= g + GAS_T + CARD_T)
    }

    /// Jump to the end card's end (a tap skips the sequence).
    pub fn skip(&mut self) {
        if self.on {
            let g = *self.gas_at.get_or_insert(self.t);
            self.t = self.t.max(g + GAS_T + CARD_T);
        }
    }

    /// Where the camera should be (world scroll in pixels), while the sequence drives it.
    pub fn camera(&self, max_scroll: f32) -> Option<f32> {
        if !self.active() {
            return None;
        }
        Some(match self.gas_at {
            // Sink with the gas, ending on the generator hall.
            Some(g) => {
                let k = ((self.t - g) / GAS_T).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                k * max_scroll
            }
            None => 0.0,
        })
    }

    /// How far the gas has sunk (world y of its front), if it has started.
    fn gas_front(&self) -> Option<f32> {
        let g = self.gas_at?;
        let k = ((self.t - g) / GAS_T).clamp(0.0, 1.0);
        Some(HUD_H + TILE + k * (WORLD_H - TILE + 10.0))
    }

    pub fn update(&mut self, dt: f32) -> Cues {
        let mut cues = Cues::default();
        if !self.on || self.done() {
            return cues;
        }
        self.t += dt;
        let t = self.t;
        // Out of the door and across the hills.
        for e in &mut self.escapees {
            if t < e.out_at || e.shot_at.is_some() {
                continue;
            }
            let d = e.target - e.x;
            e.x += d.signum() * (45.0 * dt).min(d.abs());
        }
        if t >= 0.5 && !self.cheered {
            self.cheered = true;
            cues.freed = true;
        }
        if self.gas_at.is_some_and(|g| t >= g + GAS_T) && !self.announced_end {
            self.announced_end = true;
            cues.game_over = true;
        }
        if t >= SAFEGUARD_AT && !self.alarmed {
            self.alarmed = true;
            cues.alarm = true;
        }
        if t >= DRONES_AT && self.drones.is_empty() && self.gas_at.is_none() {
            self.drones = vec![
                Drone { pos: vec2(-30.0, DRONE_Y), target: None, cooldown: 0.0 },
                Drone { pos: vec2(W + 30.0, DRONE_Y + 4.0), target: None, cooldown: 0.4 },
                Drone { pos: vec2(W * 0.5, HUD_H - 40.0), target: None, cooldown: 0.8 },
            ];
        }
        // Drones: pick a living rebel nobody else is after, hover over them, fire.
        for di in 0..self.drones.len() {
            let taken: Vec<usize> = self.drones.iter().filter_map(|d| d.target).collect();
            let d = &mut self.drones[di];
            d.cooldown -= dt;
            let alive = |i: usize| self.escapees[i].shot_at.is_none() && t >= self.escapees[i].out_at;
            if d.target.is_none_or(|i| !alive(i)) {
                d.target = (0..self.escapees.len())
                    .filter(|&i| alive(i) && !taken.contains(&i))
                    .min_by(|&a, &b| {
                        let da = (self.escapees[a].x - d.pos.x).abs();
                        let db = (self.escapees[b].x - d.pos.x).abs();
                        da.total_cmp(&db)
                    })
                    .or_else(|| (0..self.escapees.len()).find(|&i| alive(i)));
            }
            let goal = match d.target {
                Some(i) => vec2(self.escapees[i].x - 26.0, DRONE_Y),
                None => vec2(d.pos.x, HUD_H - 60.0), // nothing left: climb away
            };
            let step = goal - d.pos;
            let len = step.length();
            if len > 0.5 {
                d.pos += step / len * (DRONE_SPEED * dt).min(len);
            }
            if let Some(i) = d.target
                && len < 8.0
                && d.cooldown <= 0.0
            {
                d.cooldown = 0.7;
                let e = &mut self.escapees[i];
                e.shot_at = Some(t);
                cues.shots.push((d.pos + vec2(4.0, 4.0), vec2(e.x, ground() - 14.0)));
            }
        }
        // Everyone outside is down (or it's taking too long): gas the silo.
        let all_down = t >= DRONES_AT && self.escapees.iter().all(|e| e.shot_at.is_some());
        if self.gas_at.is_none() && ((all_down && t >= GAS_EARLIEST) || t >= GAS_LATEST) {
            self.gas_at = Some(t + 0.6);
            cues.gas_start = true;
        }
        // Vents at both walls puff gas just above the front.
        if let Some(front) = self.gas_front() {
            self.puff_t -= dt;
            if self.puff_t <= 0.0 && front < HUD_H + WORLD_H {
                self.puff_t = 0.06;
                let side = if macroquad::rand::gen_range(0, 2) == 0 { TILE * 1.1 } else { W - TILE * 1.1 };
                cues.puffs.push(vec2(side, front - 10.0));
                cues.puffs.push(vec2(macroquad::rand::gen_range(TILE * 3.2, TILE * 8.8), front));
            }
        }
        cues
    }

    /// The surface: escapees, the bodies, the drones. Drawn in world coordinates.
    pub fn draw_surface(&self, time: f32) {
        if !self.on {
            return;
        }
        let g = ground();
        for e in &self.escapees {
            if self.t < e.out_at {
                continue;
            }
            let fade = ((self.t - e.out_at) / 0.3).min(1.0);
            match e.shot_at {
                Some(s) => body(e, g, (self.t - s) / 0.35),
                None => {
                    let moving = (e.target - e.x).abs() > 1.0;
                    let mut r = sc::Enemy::for_display(e.kind, sc::vec2(e.x / TILE, (g + 4.0 - HUD_H) / TILE));
                    r.blocked = !moving;
                    r.id = (e.out_at * 100.0) as u32;
                    let facing_right = e.target >= door_x() || !moving;
                    if fade < 1.0 {
                        // Stepping out of the dark doorway.
                        draw_rectangle(e.x - 8.0, g - 30.0, 16.0, 30.0, Color::new(0.0, 0.0, 0.0, 0.4 * (1.0 - fade)));
                    }
                    render::rebel(&r, facing_right, time);
                    if !moving {
                        // Free at last: arms up.
                        let arm = (time * 7.0 + e.out_at * 5.0).sin().max(-0.2);
                        let sh = vec2(e.x + 2.0, g - 26.0);
                        let hand = sh + vec2(3.0, -9.0 - 4.0 * arm);
                        draw_line(sh.x, sh.y, hand.x, hand.y, 2.6, rgb(120, 92, 70));
                        draw_circle(hand.x, hand.y, 2.2, rgb(214, 172, 140));
                    }
                    if e.flag {
                        flag(vec2(e.x + 6.0, g - 20.0), time);
                    }
                }
            }
        }
        for d in &self.drones {
            drone(d.pos, time);
        }
    }

    /// Gas filling the silo down to its front, and the lights going out under it.
    pub fn draw_gas(&self, time: f32) {
        let Some(front) = self.gas_front() else { return };
        let top = HUD_H + TILE;
        if front <= top {
            return;
        }
        draw_rectangle(0.0, top, W, front - top, Color::new(0.02, 0.03, 0.0, 0.55));
        draw_rectangle(0.0, top, W, front - top, Color::new(0.55, 0.62, 0.18, 0.32));
        // Billowing front.
        let mut x = 0.0;
        while x < W {
            let wob = (x * 0.05 + time * 2.0).sin() * 6.0 + (x * 0.13 - time * 1.3).sin() * 4.0;
            draw_circle(x, front + wob, 16.0, Color::new(0.6, 0.68, 0.22, 0.35));
            draw_circle(x + 9.0, front + wob - 8.0, 11.0, Color::new(0.7, 0.78, 0.3, 0.25));
            x += 18.0;
        }
    }

    /// Screen-space overlays: the end card.
    pub fn draw_card(&self) {
        let Some(g) = self.gas_at else { return };
        let k = ((self.t - (g + GAS_T)) / 0.8).clamp(0.0, 1.0);
        if k <= 0.0 {
            return;
        }
        draw_rectangle(0.0, 0.0, W, PANEL_Y, Color::new(0.0, 0.0, 0.0, 0.75 * k));
        let y = HUD_H + 260.0;
        text_centered("GAME OVER", W / 2.0, y, 40.0, Color::new(1.0, 0.35, 0.28, k));
        text_centered("SAFEGUARD COMPLETE", W / 2.0, y + 32.0, 18.0, Color::new(0.85, 0.9, 0.4, k));
        text_centered(
            "You failed to prevent the Safeguard. The uprising broke out,",
            W / 2.0,
            y + 64.0,
            15.0,
            Color::new(0.8, 0.8, 0.76, k),
        );
        text_centered(
            "and Silo 1 sealed Silo 12. No one is left to remember.",
            W / 2.0,
            y + 84.0,
            15.0,
            Color::new(0.8, 0.8, 0.76, k),
        );
    }
}

/// A rebel shot down: crumples, then lies on the ground for good.
fn body(e: &Escapee, g: f32, k: f32) {
    let k = k.clamp(0.0, 1.0);
    let (r, gg, b) = match e.kind {
        sc::EnemyKind::Miner => (110, 90, 60),
        sc::EnemyKind::Porter => (90, 100, 120),
        _ => (150, 90, 50),
    };
    let tilt = k * PI / 2.0;
    let dir = vec2(tilt.sin(), -tilt.cos());
    let foot = vec2(e.x, g - 1.0);
    let hip = foot + dir * 11.0;
    let head = foot + dir * 24.0;
    draw_line(foot.x, foot.y, hip.x, hip.y, 4.0, rgb(50, 46, 44));
    draw_line(hip.x, hip.y, head.x - dir.x * 4.0, head.y - dir.y * 4.0, 7.0, rgb(r, gg, b));
    draw_circle(head.x, head.y, 3.6, rgb(200, 160, 128));
    if k >= 1.0 {
        draw_ellipse(hip.x + 2.0, g, 7.0, 1.6, 0.0, rgba(90, 20, 18, 160));
    }
    if e.flag {
        // The flag falls with them.
        draw_line(foot.x + 4.0, g - 1.0, foot.x + 30.0, g - 3.0, 1.6, rgb(90, 70, 50));
        draw_triangle(vec2(foot.x + 30.0, g - 3.0), vec2(foot.x + 42.0, g - 2.0), vec2(foot.x + 30.0, g + 1.0), rgb(170, 24, 22));
    }
}

/// A Safeguard drone: black body, four blurred rotors, a red sensor eye and a gun.
fn drone(p: Vec2, time: f32) {
    for (dx, dy) in [(-9.0, -2.0), (9.0, -2.0), (-6.0, -4.0), (6.0, -4.0)] {
        let blur = 0.5 + 0.5 * (time * 60.0 + dx).sin();
        draw_ellipse(p.x + dx, p.y + dy, 5.5, 1.2, 0.0, Color::new(0.6, 0.62, 0.66, 0.35 + 0.25 * blur));
        draw_line(p.x, p.y, p.x + dx, p.y + dy, 1.2, rgb(40, 42, 46));
    }
    draw_rectangle(p.x - 7.0, p.y - 1.5, 14.0, 5.0, rgb(28, 30, 34));
    draw_rectangle(p.x - 7.0, p.y - 1.5, 14.0, 1.2, rgb(70, 74, 80));
    draw_line(p.x + 3.0, p.y + 3.5, p.x + 7.0, p.y + 6.0, 1.6, rgb(50, 52, 56));
    let on = (time * 5.0).fract() < 0.6;
    draw_circle(p.x - 2.0, p.y + 2.0, 1.6, if on { rgb(255, 40, 30) } else { rgb(110, 20, 16) });
    if on {
        draw_circle(p.x - 2.0, p.y + 2.0, 4.0, rgba(255, 40, 30, 60));
    }
}

/// The rebels' red flag, held high.
fn flag(base: Vec2, time: f32) {
    let top = base + vec2(0.0, -26.0);
    draw_line(base.x, base.y, top.x, top.y, 1.8, rgb(90, 70, 50));
    for i in 0..5 {
        let x0 = i as f32 * 4.0;
        let w0 = (time * 6.0 - i as f32 * 0.9).sin() * 2.0;
        let w1 = (time * 6.0 - (i + 1) as f32 * 0.9).sin() * 2.0;
        let a = top + vec2(x0, w0);
        let b = top + vec2(x0 + 4.0, w1);
        let col = if i % 2 == 0 { rgb(190, 30, 26) } else { rgb(170, 24, 22) };
        draw_triangle(a, b, b + vec2(0.0, 10.0), col);
        draw_triangle(a, b + vec2(0.0, 10.0), a + vec2(0.0, 10.0), col);
    }
}
