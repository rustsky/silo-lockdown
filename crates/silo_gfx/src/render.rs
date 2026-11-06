//! Draws the silo cross-section, the spiral stair, towers and rebels.

use crate::ui::*;
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use silo_core as sc;
use silo_core::map::{AIRLOCK, COLS, PILLAR_X, PLAQUES, ROWS, SHAFT_LEFT, SHAFT_RIGHT};
use silo_core::{EnemyKind, Game, Phase, Tile, TowerKind};
use std::f32::consts::PI;

pub const TILE: f32 = 56.0;
pub const HUD_H: f32 = 72.0;
pub const FIELD_W: f32 = TILE * COLS as f32;
pub const FIELD_H: f32 = TILE * ROWS as f32;
pub const PANEL_Y: f32 = HUD_H + FIELD_H;
pub const PANEL_H: f32 = 176.0;
pub const W: f32 = FIELD_W;
pub const H: f32 = PANEL_Y + PANEL_H;

const EARTH: Color = Color::new(0.20, 0.16, 0.13, 1.0);
const CONCRETE: Color = Color::new(0.50, 0.51, 0.53, 1.0);
const STAIR_FRONT: Color = Color::new(0.72, 0.48, 0.26, 1.0);
const STAIR_BACK: Color = Color::new(0.33, 0.25, 0.19, 1.0);

/// Core tile coordinates -> logical pixels.
pub fn px(v: sc::Vec2) -> Vec2 {
    vec2(v.x * TILE, HUD_H + v.y * TILE)
}

pub fn tile_at(p: Vec2) -> Option<(i32, i32)> {
    let (c, r) = (
        (p.x / TILE).floor() as i32,
        ((p.y - HUD_H) / TILE).floor() as i32,
    );
    sc::map::in_bounds(c, r).then_some((c, r))
}

pub fn tower_color(k: TowerKind) -> Color {
    match k {
        TowerKind::Deputy => rgb(80, 145, 240),
        TowerKind::Jammer => rgb(190, 110, 250),
        TowerKind::Gas => rgb(120, 210, 100),
        TowerKind::Marksman => rgb(240, 95, 85),
    }
}

/// Static decoration generated once.
pub struct Decor {
    specks: Vec<(Vec2, f32, Color)>,
    hills: Vec<[Vec2; 3]>,
    ruins: Vec<Rect>,
    dust: Vec<(Vec2, f32)>,
    windows: Vec<(Rect, f32, bool)>,
}

impl Decor {
    pub fn new(map: &sc::Map) -> Self {
        macroquad::rand::srand(0x5110);
        let horizon = HUD_H + TILE * 0.8;
        let mut specks = vec![];
        for _ in 0..260 {
            let side = gen_range(0, 2);
            let x = if side == 0 {
                gen_range(0.0, TILE)
            } else {
                gen_range(W - TILE, W)
            };
            let y = gen_range(HUD_H + TILE * 0.85, H - PANEL_H);
            let k = gen_range(0.6, 1.5);
            specks.push((vec2(x, y), gen_range(1.0, 3.0), shade(EARTH, k)));
        }
        let mut hills = vec![];
        let mut x = -20.0;
        while x < W {
            let w = gen_range(60.0, 140.0);
            let h = gen_range(8.0, 22.0);
            hills.push([
                vec2(x, horizon),
                vec2(x + w / 2.0, horizon - h),
                vec2(x + w, horizon),
            ]);
            x += w * 0.6;
        }
        let ruins = (0..9)
            .map(|_| {
                let w = gen_range(5.0, 14.0);
                let h = gen_range(10.0, 30.0);
                let x = if gen_range(0, 2) == 0 {
                    gen_range(10.0, 230.0)
                } else {
                    gen_range(440.0, 660.0)
                };
                Rect::new(x, horizon - h, w, h)
            })
            .collect();
        let dust = (0..40)
            .map(|_| {
                (
                    vec2(gen_range(0.0, W), gen_range(HUD_H, horizon)),
                    gen_range(8.0, 30.0),
                )
            })
            .collect();
        let mut windows = vec![];
        for r in 1..ROWS {
            for c in 0..COLS {
                if map.tile(c, r) == Tile::Floor && gen_range(0, 10) < 7 {
                    let x = c as f32 * TILE + gen_range(8.0, TILE - 26.0);
                    let y = HUD_H + r as f32 * TILE + gen_range(14.0, 22.0);
                    let door = gen_range(0, 4) == 0;
                    let rect = if door {
                        Rect::new(
                            x,
                            y - 4.0,
                            14.0,
                            TILE - (y - 4.0 - HUD_H - r as f32 * TILE) - 6.0,
                        )
                    } else {
                        Rect::new(x, y, 16.0, 11.0)
                    };
                    windows.push((rect, gen_range(0.0, 10.0), door));
                }
            }
        }
        Self {
            specks,
            hills,
            ruins,
            dust,
            windows,
        }
    }
}

// ---------------------------------------------------------------- the silo

pub fn silo(game: &Game, decor: &Decor, time: f32) {
    let map = game.map();
    let top = HUD_H;
    let horizon = top + TILE * 0.8;
    let bottom = top + FIELD_H;

    // The outside: toxic haze, dead hills, ruins of an old city.
    for i in 0..12 {
        let t = i as f32 / 12.0;
        let c = Color::new(0.27 + 0.2 * t, 0.25 + 0.17 * t, 0.22 + 0.11 * t, 1.0);
        draw_rectangle(0.0, top + t * TILE * 0.8, W, TILE * 0.8 / 12.0 + 1.0, c);
    }
    for r in &decor.ruins {
        draw_rectangle(r.x, r.y, r.w, r.h, rgb(74, 66, 58));
    }
    for h in &decor.hills {
        draw_triangle(h[0], h[1], h[2], rgb(66, 56, 46));
    }
    for (p, speed) in &decor.dust {
        let x = (p.x + time * speed).rem_euclid(W);
        draw_circle(x, p.y, 1.2, rgba(170, 150, 120, 90));
    }
    draw_rectangle(0.0, horizon, W, top + TILE - horizon, rgb(78, 62, 44));

    // Earth around the silo.
    draw_rectangle(0.0, top + TILE, TILE, FIELD_H - TILE, EARTH);
    draw_rectangle(W - TILE, top + TILE, TILE, FIELD_H - TILE, EARTH);
    for (p, r, c) in &decor.specks {
        draw_circle(p.x, p.y, *r, *c);
    }

    // Cylinder interior with curvature shading.
    let (ix0, ix1) = (TILE, W - TILE);
    let strips = 48;
    for i in 0..strips {
        let t = (i as f32 + 0.5) / strips as f32;
        let k = 0.35 + 0.65 * (PI * t).sin();
        let w = (ix1 - ix0) / strips as f32;
        draw_rectangle(
            ix0 + i as f32 * w,
            top + TILE,
            w + 0.5,
            FIELD_H - TILE,
            shade(rgb(72, 74, 80), k),
        );
    }

    // Living levels: floor slabs, lit windows, doors.
    for r in 1..ROWS {
        for c in 0..COLS {
            if map.tile(c, r) != Tile::Floor {
                continue;
            }
            let y = top + (r + 1) as f32 * TILE;
            draw_rectangle(c as f32 * TILE, y - 7.0, TILE, 7.0, rgb(96, 98, 104));
            draw_line(
                c as f32 * TILE,
                y - 7.0,
                (c + 1) as f32 * TILE,
                y - 7.0,
                1.0,
                rgb(140, 142, 148),
            );
        }
    }
    for (rect, phase, door) in &decor.windows {
        if *door {
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, rgb(34, 30, 28));
            draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.5, rgb(90, 84, 76));
        } else {
            let flicker =
                0.75 + 0.25 * ((time * 3.0 + phase).sin() * (time * 7.3 + phase * 2.0).sin()).abs();
            draw_rectangle(
                rect.x - 3.0,
                rect.y - 3.0,
                rect.w + 6.0,
                rect.h + 6.0,
                rgba(255, 190, 90, (40.0 * flicker) as u8),
            );
            draw_rectangle(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                rgba(255, 205, 120, (200.0 * flicker) as u8),
            );
            draw_line(
                rect.x + rect.w / 2.0,
                rect.y,
                rect.x + rect.w / 2.0,
                rect.y + rect.h,
                1.5,
                rgb(60, 50, 40),
            );
        }
    }

    // The open stair shaft.
    let (sx0, sx1) = (SHAFT_LEFT.floor() * TILE, SHAFT_RIGHT.ceil() * TILE);
    for i in 0..24 {
        let t = (i as f32 + 0.5) / 24.0;
        let k = 0.55 + 0.45 * (PI * t).sin();
        let w = (sx1 - sx0) / 24.0;
        draw_rectangle(
            sx0 + i as f32 * w,
            top + TILE,
            w + 0.5,
            FIELD_H - TILE,
            shade(rgb(20, 22, 30), k),
        );
    }
    // Catwalk ledges on the shaft edges at each level.
    for r in 1..ROWS {
        let y = top + (r + 1) as f32 * TILE;
        for x in [sx0, sx1] {
            draw_line(x, top + r as f32 * TILE, x, y, 2.0, rgb(52, 54, 60));
        }
    }

    // Silo wall: concrete face and domed shoulders.
    let wall = 6.0;
    let shoulder = [(
        vec2(TILE, top + TILE),
        vec2(2.0 * TILE, top + TILE),
        vec2(TILE, top + 2.0 * TILE),
    )];
    for (a, b, c) in shoulder {
        draw_triangle(a, b, c, EARTH);
        let m = |v: Vec2| vec2(W - v.x, v.y);
        draw_triangle(m(a), m(b), m(c), EARTH);
        draw_line(b.x, b.y, c.x, c.y, wall, CONCRETE);
        draw_line(m(b).x, m(b).y, m(c).x, m(c).y, wall, CONCRETE);
    }
    draw_line(TILE, top + 2.0 * TILE, TILE, bottom, wall, CONCRETE);
    draw_line(W - TILE, top + 2.0 * TILE, W - TILE, bottom, wall, CONCRETE);
    draw_line(
        2.0 * TILE - 2.0,
        top + TILE,
        W - 2.0 * TILE + 2.0,
        top + TILE,
        wall,
        CONCRETE,
    );
    // Roof slab at ground level.
    draw_rectangle(
        TILE * 0.85,
        horizon - 3.0,
        W - TILE * 1.7,
        top + TILE - horizon + 3.0,
        rgb(104, 104, 100),
    );
    draw_line(
        TILE * 0.85,
        horizon - 3.0,
        W - TILE * 0.85,
        horizon - 3.0,
        2.0,
        rgb(150, 150, 144),
    );

    // Level markers carved into the earth.
    for &(row, level, name) in PLAQUES {
        let y = top + row as f32 * TILE + 22.0;
        draw_rectangle(4.0, y - 16.0, TILE - 12.0, 22.0, rgba(0, 0, 0, 90));
        text_centered(
            &format!("{level}"),
            TILE / 2.0 - 2.0,
            y,
            18.0,
            rgb(220, 184, 92),
        );
        let short = match name {
            "MECHANICAL" => "MECH",
            "DOWN DEEP" => "DEEP",
            "SUPPLY" => "SUPPLY",
            other => other,
        };
        draw_rectangle(
            W - TILE + 6.0,
            y - 16.0,
            TILE - 10.0,
            22.0,
            rgba(0, 0, 0, 90),
        );
        text_centered(short, W - TILE / 2.0 + 1.0, y, 13.0, rgb(220, 184, 92));
    }
}

// ---------------------------------------------------------------- spiral stair

fn stair(a: sc::Vec2, b: sc::Vec2, front: bool) {
    let (pa, pb) = (px(a), px(b));
    let color = if front { STAIR_FRONT } else { STAIR_BACK };
    if (pa.x - pb.x).abs() < 0.5 {
        // Hatch ladder up to the airlock.
        for dx in [-9.0, 9.0] {
            draw_line(pa.x + dx, pa.y, pb.x + dx, pb.y, 3.0, color);
        }
        let mut y = pb.y + 4.0;
        while y < pa.y {
            draw_line(pa.x - 9.0, y, pa.x + 9.0, y, 2.0, shade(color, 0.8));
            y += 8.0;
        }
        return;
    }
    let th = if front { 10.0 } else { 8.0 };
    draw_line(pa.x, pa.y + 6.0, pb.x, pb.y + 6.0, th, rgba(0, 0, 0, 90)); // shadow
    draw_line(pa.x, pa.y, pb.x, pb.y, th, color);
    // Treads.
    let len = pa.distance(pb);
    let dir = (pb - pa) / len;
    let mut d = 4.0;
    while d < len {
        let p = pa + dir * d;
        draw_line(
            p.x,
            p.y - th / 2.0,
            p.x,
            p.y + th / 2.0,
            1.5,
            shade(color, 0.65),
        );
        d += 7.0;
    }
    // Handrail with posts.
    let rail = vec2(0.0, -18.0);
    let rail_c = if front {
        rgb(150, 150, 156)
    } else {
        rgb(70, 70, 76)
    };
    draw_line(
        pa.x + rail.x,
        pa.y + rail.y,
        pb.x + rail.x,
        pb.y + rail.y,
        2.0,
        rail_c,
    );
    let mut d = 0.0;
    while d <= len {
        let p = pa + dir * d;
        draw_line(p.x, p.y, p.x, p.y + rail.y, 1.5, rail_c);
        d += 24.0;
    }
    // Landings at the turns.
    for p in [pa, pb] {
        draw_rectangle(p.x - 16.0, p.y - th / 2.0, 32.0, th, shade(color, 0.9));
    }
}

fn pillar() {
    let cx = PILLAR_X * TILE;
    let (y0, y1) = (HUD_H + 1.4 * TILE, HUD_H + FIELD_H);
    let w = 30.0;
    for i in 0..10 {
        let t = (i as f32 + 0.5) / 10.0;
        let k = 0.35 + 0.65 * (PI * t).sin();
        draw_rectangle(
            cx - w / 2.0 + t * w - w / 20.0,
            y0,
            w / 10.0 + 0.5,
            y1 - y0,
            shade(rgb(110, 112, 118), k),
        );
    }
    let mut y = y0 + TILE;
    while y < y1 {
        draw_line(cx - w / 2.0, y, cx + w / 2.0, y, 2.0, rgb(50, 52, 56));
        y += TILE;
    }
}

// ---------------------------------------------------------------- airlock

pub fn airlock(game: &Game, time: f32, flash: f32) {
    let c = vec2((AIRLOCK[0].0 as f32 + 1.0) * TILE, HUD_H + TILE * 0.48);
    let lost = game.phase() == Phase::Lost;
    // Bunker housing.
    draw_rectangle(c.x - 74.0, c.y - 24.0, 148.0, 52.0, rgb(92, 92, 90));
    draw_rectangle_lines(c.x - 74.0, c.y - 24.0, 148.0, 52.0, 2.0, rgb(130, 130, 126));
    // Hazard stripes.
    for i in 0..12 {
        let x = c.x - 72.0 + i as f32 * 12.0;
        let col = if i % 2 == 0 {
            rgb(210, 170, 40)
        } else {
            rgb(30, 30, 30)
        };
        draw_rectangle(x, c.y + 20.0, 12.0, 6.0, col);
    }
    let r = 21.0;
    if lost {
        // Light from the outside floods in.
        for i in 0..7 {
            let a = -PI / 2.0 + (i as f32 - 3.0) * 0.25 + (time * 0.5).sin() * 0.05;
            let tip = c + vec2(a.cos(), a.sin()) * 120.0;
            let side = vec2(-a.sin(), a.cos()) * 18.0;
            draw_triangle(c, tip + side, tip - side, rgba(255, 250, 220, 40));
        }
        draw_circle(c.x, c.y, r, rgb(250, 248, 232));
        draw_circle(c.x + 40.0, c.y, r * 0.95, rgb(120, 122, 126)); // door swung open
        draw_circle_lines(c.x + 40.0, c.y, r * 0.95, 3.0, rgb(70, 72, 76));
    } else {
        draw_circle(c.x, c.y, r + 5.0, rgb(50, 52, 56));
        draw_circle(c.x, c.y, r, rgb(140, 144, 150));
        draw_circle_lines(c.x, c.y, r * 0.7, 2.0, rgb(90, 94, 100));
        for i in 0..4 {
            let a = i as f32 * PI / 2.0 + PI / 4.0;
            draw_line(
                c.x,
                c.y,
                c.x + a.cos() * r * 0.7,
                c.y + a.sin() * r * 0.7,
                3.0,
                rgb(96, 100, 106),
            );
        }
        draw_circle(c.x, c.y, 5.0, rgb(70, 72, 76));
    }
    // Seal lights: one per remaining seal.
    let n = sc::stats::START_SEALS;
    for i in 0..n {
        let x = c.x - 66.0 + i as f32 * (132.0 / (n - 1) as f32);
        let y = c.y - 17.0;
        let lit = i < game.seals();
        let col = if !lit {
            rgb(40, 20, 20)
        } else if game.seals() > n / 2 {
            rgb(90, 230, 110)
        } else if game.seals() > n / 4 {
            rgb(240, 200, 60)
        } else {
            rgb(255, 70, 50)
        };
        draw_circle(x, y, 2.6, col);
    }
    if flash > 0.0 {
        draw_circle(
            c.x,
            c.y,
            r + 14.0,
            rgba(255, 40, 30, (flash * 300.0).min(160.0) as u8),
        );
    }
}

// ---------------------------------------------------------------- towers

pub fn tower_icon(kind: TowerKind, c: Vec2, level: u8, aim: Option<Vec2>, recoil: f32, alpha: f32) {
    let col = with_alpha(tower_color(kind), alpha);
    let dark = with_alpha(rgb(34, 38, 44), alpha);
    let steel = with_alpha(rgb(120, 126, 136), alpha);
    draw_poly(
        c.x,
        c.y + 4.0,
        8,
        22.0,
        22.5,
        with_alpha(rgba(0, 0, 0, 120), alpha),
    );
    draw_poly(c.x, c.y, 8, 21.0, 22.5, dark);
    draw_poly_lines(c.x, c.y, 8, 21.0, 22.5, 2.0, steel);
    let angle = aim.map_or(-PI / 2.0 - 0.5, |a| (a.y - c.y).atan2(a.x - c.x));
    let dir = vec2(angle.cos(), angle.sin());
    let back = recoil * 4.0;
    match kind {
        TowerKind::Deputy => {
            let tip = c + dir * (20.0 - back);
            draw_line(c.x, c.y, tip.x, tip.y, 6.0, steel);
            draw_circle(c.x, c.y, 11.0, col);
            draw_circle(c.x, c.y, 5.0, shade(col, 0.6));
        }
        TowerKind::Jammer => {
            draw_rectangle(c.x - 10.0, c.y - 6.0, 20.0, 14.0, col);
            draw_line(c.x, c.y - 6.0, c.x, c.y - 24.0, 2.5, steel);
            draw_circle(c.x, c.y - 24.0, 3.0, with_alpha(rgb(255, 120, 120), alpha));
            for i in 1..=2 {
                let rr = 6.0 * i as f32;
                draw_circle_lines(c.x, c.y - 24.0, rr, 1.2, with_alpha(col, 0.5 * alpha));
            }
        }
        TowerKind::Gas => {
            let tip = c + dir * (14.0 - back);
            draw_line(c.x, c.y, tip.x, tip.y, 11.0, shade(col, 0.7));
            draw_circle(c.x, c.y, 12.0, col);
            for dx in [-13.0, 13.0] {
                draw_circle(
                    c.x + dx,
                    c.y + 12.0,
                    4.0,
                    with_alpha(rgb(220, 200, 60), alpha),
                );
            }
        }
        TowerKind::Marksman => {
            let tip = c + dir * (27.0 - back);
            draw_line(c.x, c.y, tip.x, tip.y, 3.5, steel);
            draw_circle(c.x, c.y, 9.0, col);
            let scope = c + dir * 8.0 + vec2(-dir.y, dir.x) * 5.0;
            draw_circle(scope.x, scope.y, 3.0, dark);
        }
    }
    for i in 0..level {
        let x = c.x - (level as f32 - 1.0) * 5.0 + i as f32 * 10.0;
        draw_circle(x, c.y + 20.0, 3.2, with_alpha(rgb(250, 210, 70), alpha));
        draw_circle_lines(x, c.y + 20.0, 3.2, 1.0, with_alpha(rgb(80, 60, 10), alpha));
    }
}

fn towers(game: &Game) {
    let map = game.map();
    for t in game.towers() {
        let c = px(t.pos);
        if map.tile(t.col, t.row) == Tile::Shaft {
            // Catwalk platform bolted into the shaft.
            let (x, y) = (c.x - TILE / 2.0 + 3.0, c.y + 14.0);
            draw_rectangle(x, y, TILE - 6.0, 6.0, rgb(70, 72, 78));
            let mut gx = x;
            while gx < x + TILE - 6.0 {
                draw_line(gx, y, gx + 4.0, y + 6.0, 1.0, rgb(40, 42, 46));
                gx += 5.0;
            }
            draw_line(x + 6.0, y + 6.0, x + 2.0, y + 22.0, 2.0, rgb(60, 62, 66));
            draw_line(
                x + TILE - 12.0,
                y + 6.0,
                x + TILE - 8.0,
                y + 22.0,
                2.0,
                rgb(60, 62, 66),
            );
        }
        let s = t.stats();
        let since = s.cooldown - t.cooldown; // seconds since last shot
        let recoil = (1.0 - since / 0.12).clamp(0.0, 1.0) * (t.cooldown > 0.0) as u8 as f32;
        tower_icon(t.kind, c, t.level, t.aim.map(px), recoil, 1.0);
        if recoil > 0.5 && matches!(t.kind, TowerKind::Deputy | TowerKind::Marksman) {
            let a = t.aim.map(px).unwrap_or(c);
            let dir = (a - c).normalize_or_zero();
            let tip = c + dir * 24.0;
            draw_circle(tip.x, tip.y, 5.0, rgba(255, 230, 140, 220));
        }
    }
}

// ---------------------------------------------------------------- rebels

fn rebel(e: &sc::Enemy, front: bool, time: f32) {
    let s =
        1.35 * if e.kind == EnemyKind::Leader {
            1.35
        } else {
            1.0
        } * if front { 1.0 } else { 0.85 };
    let k = if front { 1.0 } else { 0.62 };
    let base = px(e.pos) + vec2(e.lane * TILE * 0.9, -4.0);
    let phase = time * 11.0 + e.id as f32 * 1.7;
    let bob = phase.sin().abs() * 1.5 * s;
    let feet = base - vec2(0.0, bob);
    let hip = feet - vec2(0.0, 8.0 * s);
    let stride = phase.sin() * 3.0 * s;
    let pants = shade(rgb(50, 46, 44), k);
    draw_line(hip.x, hip.y, feet.x - stride, feet.y + bob, 2.6 * s, pants);
    draw_line(hip.x, hip.y, feet.x + stride, feet.y + bob, 2.6 * s, pants);
    let shirt = shade(
        match e.kind {
            EnemyKind::Porter => rgb(200, 176, 128),
            EnemyKind::Mechanic => rgb(226, 120, 40),
            EnemyKind::Miner => rgb(122, 92, 62),
            EnemyKind::Leader => rgb(196, 40, 36),
        },
        k,
    );
    let (bw, bh) = (9.0 * s, 11.0 * s);
    draw_rectangle(hip.x - bw / 2.0, hip.y - bh, bw, bh, shirt);
    let head = hip - vec2(0.0, bh + 4.0 * s);
    draw_circle(head.x, head.y, 4.2 * s, shade(rgb(226, 186, 150), k));
    match e.kind {
        EnemyKind::Porter => {
            draw_rectangle(
                hip.x - bw / 2.0 - 4.0 * s,
                hip.y - bh + 1.0,
                4.0 * s,
                8.0 * s,
                shade(rgb(110, 80, 50), k),
            );
        }
        EnemyKind::Mechanic => {
            let hand = hip + vec2(6.0 * s, -5.0 * s);
            draw_line(
                hand.x,
                hand.y,
                hand.x + 4.0 * s,
                hand.y - 7.0 * s,
                2.0 * s,
                shade(rgb(170, 170, 176), k),
            );
        }
        EnemyKind::Miner => {
            draw_rectangle(
                head.x - 5.0 * s,
                head.y - 5.0 * s,
                10.0 * s,
                3.0 * s,
                shade(rgb(230, 190, 40), k),
            );
            draw_circle(
                head.x + 4.0 * s,
                head.y - 3.5 * s,
                1.6 * s,
                rgb(255, 250, 200),
            );
        }
        EnemyKind::Leader => {
            let pole = hip + vec2(-6.0 * s, -bh);
            draw_line(
                pole.x,
                pole.y + 8.0,
                pole.x,
                pole.y - 18.0 * s,
                1.6 * s,
                shade(rgb(150, 130, 100), k),
            );
            let wave = (time * 6.0).sin() * 2.0;
            draw_triangle(
                vec2(pole.x, pole.y - 18.0 * s),
                vec2(pole.x - 14.0 * s, pole.y - 14.0 * s + wave),
                vec2(pole.x, pole.y - 9.0 * s),
                shade(rgb(230, 40, 30), k),
            );
            draw_rectangle(
                head.x - 4.5 * s,
                head.y - 2.0 * s,
                9.0 * s,
                2.0 * s,
                shade(rgb(255, 60, 40), k),
            );
        }
    }
    if e.is_slowed() {
        for i in 0..3 {
            let a = time * 5.0 + i as f32 * 2.09;
            let p = head + vec2(a.cos() * 8.0 * s, a.sin() * 3.0 * s - 5.0);
            draw_circle(p.x, p.y, 1.6, rgb(220, 150, 255));
        }
    }
    if e.hp < e.max_hp {
        let w = 18.0 * s;
        let y = head.y - 9.0 * s;
        draw_rectangle(head.x - w / 2.0, y, w, 3.0, rgba(0, 0, 0, 180));
        let f = (e.hp / e.max_hp).clamp(0.0, 1.0);
        let c = if f > 0.5 {
            rgb(120, 220, 90)
        } else if f > 0.25 {
            rgb(240, 200, 60)
        } else {
            rgb(240, 70, 50)
        };
        draw_rectangle(head.x - w / 2.0, y, w * f, 3.0, c);
    }
}

fn projectiles(game: &Game) {
    for p in game.projectiles() {
        let pos = px(p.pos);
        let dest = px(p.dest);
        match p.kind {
            sc::ProjectileKind::Bullet => {
                let dir = (dest - pos).normalize_or_zero();
                let tail = pos - dir * 9.0;
                draw_line(tail.x, tail.y, pos.x, pos.y, 2.0, rgba(255, 230, 140, 160));
                draw_circle(pos.x, pos.y, 2.5, rgb(255, 245, 190));
            }
            sc::ProjectileKind::Shell => {
                draw_circle(pos.x, pos.y + 6.0, 4.0, rgba(0, 0, 0, 80));
                draw_circle(pos.x, pos.y, 5.0, rgb(150, 220, 90));
                draw_circle_lines(pos.x, pos.y, 5.0, 1.5, rgb(50, 90, 30));
            }
        }
    }
}

/// Everything inside the field, in depth order.
pub fn world(game: &Game, decor: &Decor, time: f32, flash: f32) {
    let map = game.map();
    silo(game, decor, time);
    let mut enemies: Vec<&sc::Enemy> = game.enemies().iter().collect();
    enemies.sort_by(|a, b| a.dist.total_cmp(&b.dist));

    for (a, b, front) in map.segments() {
        if !front {
            stair(a, b, false);
        }
    }
    for e in enemies.iter().filter(|e| !map.is_front(e.dist)) {
        rebel(e, false, time);
    }
    pillar();
    for (a, b, front) in map.segments() {
        if front {
            stair(a, b, true);
        }
    }
    airlock(game, time, flash);
    towers(game);
    for e in enemies.iter().filter(|e| map.is_front(e.dist)) {
        rebel(e, true, time);
    }
    projectiles(game);
}

/// Soft dark vignette over the field edges to sell the underground mood.
pub fn vignette() {
    let steps = 10;
    for i in 0..steps {
        let a = (1.0 - i as f32 / steps as f32) * 0.05;
        let w = 6.0;
        let x = i as f32 * w;
        draw_rectangle(x, HUD_H, w, FIELD_H, Color::new(0.0, 0.0, 0.0, a * 3.0));
        draw_rectangle(
            W - x - w,
            HUD_H,
            w,
            FIELD_H,
            Color::new(0.0, 0.0, 0.0, a * 3.0),
        );
        draw_rectangle(
            0.0,
            PANEL_Y - x - w,
            W,
            w,
            Color::new(0.0, 0.0, 0.0, a * 4.0),
        );
    }
}

pub fn range_circle(c: Vec2, r: f32, ok: bool) {
    let col = if ok {
        rgba(120, 230, 140, 255)
    } else {
        rgba(255, 90, 80, 255)
    };
    draw_circle(c.x, c.y, r, with_alpha(col, 0.10));
    draw_circle_lines(c.x, c.y, r, 2.0, with_alpha(col, 0.7));
}
