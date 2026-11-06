//! Draws the silo cross-section, the spiral stair, towers and rebels.

use crate::ui::*;
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use silo_core as sc;
use silo_core::map::{
    AIRLOCK, COLS, PILLAR_X, PLAQUES, ROWS, SECTIONS, SHAFT_LEFT, SHAFT_RIGHT, VIEW_ROWS,
    level_of_row,
};
use silo_core::{EnemyKind, Game, Phase, Tile, TowerKind};
use std::f32::consts::PI;

pub const TILE: f32 = 56.0;
pub const HUD_H: f32 = 72.0;
pub const FIELD_W: f32 = TILE * COLS as f32;
/// Height of the on-screen field: one section of the silo.
pub const FIELD_H: f32 = TILE * VIEW_ROWS as f32;
/// Height of the whole silo in world pixels; the field scrolls over it.
pub const WORLD_H: f32 = TILE * ROWS as f32;
pub const PANEL_Y: f32 = HUD_H + FIELD_H;
pub const PANEL_H: f32 = 176.0;
pub const W: f32 = FIELD_W;
pub const H: f32 = PANEL_Y + PANEL_H;

const EARTH: Color = Color::new(0.20, 0.16, 0.13, 1.0);
const CONCRETE: Color = Color::new(0.50, 0.51, 0.53, 1.0);
const STAIR_FRONT: Color = Color::new(0.72, 0.48, 0.26, 1.0);
const STAIR_BACK: Color = Color::new(0.33, 0.25, 0.19, 1.0);

/// Core tile coordinates -> world pixels (screen pixels when the view is at the top).
pub fn px(v: sc::Vec2) -> Vec2 {
    vec2(v.x * TILE, HUD_H + v.y * TILE)
}

/// World pixel -> tile.
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
        for _ in 0..260 * 3 {
            let side = gen_range(0, 2);
            let x = if side == 0 {
                gen_range(0.0, TILE)
            } else {
                gen_range(W - TILE, W)
            };
            let y = gen_range(HUD_H + TILE * 0.85, HUD_H + WORLD_H);
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
    let bottom = top + WORLD_H;

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
    draw_rectangle(0.0, top + TILE, TILE, WORLD_H - TILE, EARTH);
    draw_rectangle(W - TILE, top + TILE, TILE, WORLD_H - TILE, EARTH);
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
            WORLD_H - TILE,
            shade(rgb(72, 74, 80), k),
        );
    }

    // Living levels: floor slabs, lit windows, doors.
    for r in 1..ROWS {
        for c in 0..COLS {
            if !matches!(
                map.tile(c, r),
                Tile::Floor | Tile::ItRoom | Tile::SupplyRoom
            ) {
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
            WORLD_H - TILE,
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

    // Section boundaries: a heavy slab with hazard paint, and a sign on each section's top level.
    for (i, &(name, first, last)) in SECTIONS.iter().enumerate() {
        let y = top + first as f32 * TILE;
        if i > 0 {
            draw_rectangle(TILE, y - 5.0, W - 2.0 * TILE, 10.0, rgb(62, 62, 64));
            let mut x = TILE;
            while x < W - TILE {
                draw_triangle(
                    vec2(x, y + 5.0),
                    vec2(x + 10.0, y - 5.0),
                    vec2(x + 18.0, y - 5.0),
                    rgba(200, 160, 40, 150),
                );
                x += 22.0;
            }
        }
        let row = first.max(2);
        let sy = top + row as f32 * TILE + 8.0;
        let label = format!("{name}  {}-{}", level_of_row(first), level_of_row(last) + 3);
        let w = measure(&label, 12.0) + 14.0;
        draw_rectangle(TILE + 6.0, sy, w, 18.0, rgb(30, 34, 40));
        draw_rectangle_lines(TILE + 6.0, sy, w, 18.0, 1.5, rgb(200, 160, 60));
        text(&label, TILE + 13.0, sy + 13.5, 12.0, rgb(230, 196, 100));
    }

    // Level numbers carved into the earth, every other row, with named places below them.
    let gold = rgb(220, 184, 92);
    for row in (2..ROWS).step_by(2) {
        let y = top + row as f32 * TILE + 22.0;
        draw_rectangle(4.0, y - 16.0, TILE - 12.0, 22.0, rgba(0, 0, 0, 90));
        text_centered(
            &format!("{}", level_of_row(row)),
            TILE / 2.0 - 2.0,
            y,
            18.0,
            gold,
        );
    }
    for &(row, name) in PLAQUES {
        let y = top + row as f32 * TILE + 40.0;
        text_centered(name, TILE / 2.0 - 2.0, y, 11.0, shade(gold, 0.8));
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
    let (y0, y1) = (HUD_H + 1.4 * TILE, HUD_H + WORLD_H);
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
        if t.hp < t.max_hp {
            hp_bar(vec2(c.x, c.y - 27.0), 34.0, t.hp / t.max_hp);
        }
        if recoil > 0.5 && matches!(t.kind, TowerKind::Deputy | TowerKind::Marksman) {
            let a = t.aim.map(px).unwrap_or(c);
            let dir = (a - c).normalize_or_zero();
            let tip = c + dir * 24.0;
            draw_circle(tip.x, tip.y, 5.0, rgba(255, 230, 140, 220));
        }
    }
}

fn hp_bar(center: Vec2, w: f32, f: f32) {
    let f = f.clamp(0.0, 1.0);
    draw_rectangle(
        center.x - w / 2.0 - 1.0,
        center.y - 1.0,
        w + 2.0,
        5.0,
        rgba(0, 0, 0, 200),
    );
    let c = if f > 0.5 {
        rgb(120, 220, 90)
    } else if f > 0.25 {
        rgb(240, 200, 60)
    } else {
        rgb(240, 70, 50)
    };
    draw_rectangle(center.x - w / 2.0, center.y, w * f, 3.0, c);
}

// ---------------------------------------------------------------- IT

/// IT's server room: racks with blinking lights, or a dead, sparking room once it collapses.
fn it_room(game: &Game, time: f32) {
    let (c0, r) = sc::map::IT_ROOM[0];
    let x = c0 as f32 * TILE;
    let y = HUD_H + r as f32 * TILE;
    let w = TILE * sc::map::IT_ROOM.len() as f32;
    let down = game.cameras_down();
    draw_rectangle(x + 2.0, y + 4.0, w - 4.0, TILE - 11.0, rgb(18, 22, 30));
    draw_rectangle_lines(
        x + 2.0,
        y + 4.0,
        w - 4.0,
        TILE - 11.0,
        2.0,
        rgb(70, 110, 160),
    );
    for i in 0..4 {
        let rx = x + 10.0 + i as f32 * 24.0;
        draw_rectangle(rx, y + 14.0, 16.0, TILE - 22.0, rgb(40, 44, 52));
        for j in 0..5 {
            let ly = y + 18.0 + j as f32 * 5.5;
            let on = !down && ((time * 6.0 + (i * 7 + j * 3) as f32).sin() > -0.3);
            let c = if down {
                rgb(40, 20, 20)
            } else if (i + j) % 3 == 0 {
                rgb(90, 170, 255)
            } else {
                rgb(90, 240, 120)
            };
            draw_rectangle(rx + 3.0, ly, 3.0, 2.0, if on { c } else { rgb(30, 34, 40) });
            draw_rectangle(rx + 8.0, ly, 5.0, 1.5, rgb(70, 74, 82));
        }
    }
    let sign = rgb(110, 170, 240);
    draw_rectangle(x + w / 2.0 - 16.0, y - 4.0, 32.0, 14.0, rgb(20, 26, 36));
    text_centered(
        "IT",
        x + w / 2.0,
        y + 7.0,
        13.0,
        if down { rgb(200, 70, 60) } else { sign },
    );
    if down {
        // Sparks and smoke.
        for i in 0..6 {
            let t = (time * 2.0 + i as f32 * 0.37).fract();
            let p = vec2(x + 14.0 + (i * 17 % 80) as f32, y + 18.0 - t * 16.0);
            draw_circle(
                p.x,
                p.y,
                1.5 + t * 3.0,
                rgba(140, 130, 120, ((1.0 - t) * 150.0) as u8),
            );
        }
        if (time * 9.0).sin() > 0.6 {
            draw_circle(x + 40.0, y + 26.0, 3.0, rgb(255, 230, 120));
        }
        let label = format!("OFFLINE {:.0}s", game.it_down().ceil());
        draw_rectangle(
            x + 4.0,
            y + TILE - 22.0,
            w - 8.0,
            14.0,
            rgba(60, 10, 10, 220),
        );
        text_centered(
            &label,
            x + w / 2.0,
            y + TILE - 11.0,
            11.0,
            rgb(255, 120, 100),
        );
    } else if game.it_hp() < sc::stats::IT_HP {
        hp_bar(
            vec2(x + w / 2.0, y + 12.0),
            w - 24.0,
            game.it_hp() / sc::stats::IT_HP,
        );
    }
}

/// Supply's depot: crates, and once Supply has joined the uprising, open crates of guns and a
/// rebel flag.
fn supply_room(game: &Game, time: f32) {
    let (c0, r) = sc::map::SUPPLY_ROOM[0];
    let x = c0 as f32 * TILE;
    let y = HUD_H + r as f32 * TILE;
    let w = TILE * sc::map::SUPPLY_ROOM.len() as f32;
    let joined = game.supply_joined();
    draw_rectangle(x + 2.0, y + 4.0, w - 4.0, TILE - 11.0, rgb(34, 30, 24));
    let edge = if joined {
        rgb(200, 70, 50)
    } else {
        rgb(150, 120, 70)
    };
    draw_rectangle_lines(x + 2.0, y + 4.0, w - 4.0, TILE - 11.0, 2.0, edge);
    // Crate stacks.
    for (i, (dx, dy, cw, ch)) in [
        (8.0, 28.0, 22.0, 17.0),
        (32.0, 28.0, 22.0, 17.0),
        (18.0, 13.0, 20.0, 15.0),
        (62.0, 25.0, 30.0, 20.0),
    ]
    .iter()
    .enumerate()
    {
        let (cx, cy) = (x + dx, y + dy);
        draw_rectangle(cx, cy, *cw, *ch, rgb(120, 92, 54));
        draw_rectangle_lines(cx, cy, *cw, *ch, 1.5, rgb(70, 52, 30));
        draw_line(cx, cy, cx + cw, cy + ch, 1.0, rgb(80, 60, 36));
        if joined && i == 3 {
            // Lid off: rifles inside.
            for j in 0..3 {
                let gx = cx + 4.0 + j as f32 * 8.0;
                draw_line(gx, cy + 2.0, gx + 4.0, cy - 9.0, 2.0, rgb(40, 38, 36));
            }
        }
    }
    let sign = "SUPPLY";
    draw_rectangle(x + w / 2.0 - 28.0, y - 4.0, 56.0, 14.0, rgb(30, 26, 20));
    text_centered(
        sign,
        x + w / 2.0,
        y + 7.0,
        12.0,
        if joined {
            rgb(240, 110, 80)
        } else {
            rgb(220, 184, 92)
        },
    );
    if joined {
        // Rebel flag on the depot.
        let pole = vec2(x + w - 12.0, y + 8.0);
        draw_line(
            pole.x,
            pole.y,
            pole.x,
            pole.y + 34.0,
            1.5,
            rgb(150, 130, 100),
        );
        let wave = (time * 5.0).sin() * 2.0;
        draw_triangle(
            pole,
            vec2(pole.x - 16.0, pole.y + 5.0 + wave),
            vec2(pole.x, pole.y + 11.0),
            rgb(220, 40, 30),
        );
    }
}

/// A barricade of crates and sandbags across the stair; cracks as it weakens.
pub fn barrier_icon(c: Vec2, s: f32, hp: f32, alpha: f32) {
    let a = |col: Color| with_alpha(col, alpha);
    let (w, h) = (40.0 * s, 30.0 * s);
    let base = c + vec2(0.0, 6.0 * s);
    draw_rectangle(base.x - w / 2.0, base.y - h, w, h, a(rgb(110, 84, 52)));
    for i in 0..3 {
        let yy = base.y - h + (i as f32 + 0.5) * h / 3.0;
        draw_line(
            base.x - w / 2.0,
            yy,
            base.x + w / 2.0,
            yy,
            1.5 * s,
            a(rgb(70, 52, 30)),
        );
    }
    for dx in [-w / 2.0 + 3.0 * s, w / 2.0 - 3.0 * s] {
        draw_line(
            base.x + dx,
            base.y - h,
            base.x + dx,
            base.y,
            3.0 * s,
            a(rgb(60, 62, 66)),
        );
    }
    // Sandbags on top.
    for i in 0..4 {
        let x = base.x - w / 2.0 + 5.0 * s + i as f32 * 10.0 * s;
        draw_circle(x, base.y - h, 5.0 * s, a(rgb(170, 150, 110)));
    }
    let hp = hp.clamp(0.0, 1.0);
    if hp < 0.66 {
        draw_line(
            base.x - 8.0 * s,
            base.y - h + 4.0,
            base.x + 2.0 * s,
            base.y - 8.0 * s,
            1.5,
            a(rgb(30, 22, 14)),
        );
    }
    if hp < 0.33 {
        draw_line(
            base.x + 10.0 * s,
            base.y - h + 6.0,
            base.x - 2.0 * s,
            base.y - 4.0,
            1.5,
            a(rgb(30, 22, 14)),
        );
    }
    if hp < 1.0 {
        hp_bar(vec2(base.x, base.y - h - 12.0 * s), w, hp);
    }
}

/// What IT can't see: everything below Up Top while the server room is down, and floors whose
/// power was cut. Drawn over the whole world, so rebels there are hidden.
pub fn darkness(game: &Game, time: f32) {
    let band = |first: i32, last: i32| {
        (
            HUD_H + first as f32 * TILE,
            (last - first + 1) as f32 * TILE,
        )
    };
    for b in game.blackouts() {
        let (y, h) = band(b.first, b.last);
        let flick = if b.time_left < 0.6 && (time * 30.0).sin() > 0.0 {
            0.8
        } else {
            0.99
        };
        draw_rectangle(
            TILE,
            y,
            W - 2.0 * TILE,
            h,
            Color::new(0.01, 0.01, 0.02, flick),
        );
        let cy = y + h / 2.0;
        let name = SECTIONS[sc::map::section_of_row(b.first)].0;
        text_centered("NO POWER", W / 2.0, cy - 6.0, 34.0, rgba(150, 130, 90, 220));
        text_centered(
            &format!("{name} IS DARK  {:.0}s", b.time_left.ceil()),
            W / 2.0,
            cy + 24.0,
            16.0,
            rgba(140, 140, 140, 220),
        );
    }
    if game.cameras_down() {
        let first = SECTIONS[1].1;
        let (y, h) = band(first, ROWS - 1);
        draw_rectangle(0.0, y, W, h, rgb(10, 11, 13));
        // Camera static.
        let seed = (time * 20.0) as u32;
        for i in 0..900u32 {
            let n = (i.wrapping_mul(2_654_435_761) ^ seed.wrapping_mul(40_503)).rotate_left(i % 17);
            let px = (n % W as u32) as f32;
            let py = y + ((n >> 10) % h as u32) as f32;
            let g = 40 + (n >> 24) % 60;
            draw_rectangle(px, py, 3.0, 2.0, rgb(g as u8, g as u8, g as u8));
        }
        for (_, first, last) in &SECTIONS[1..] {
            let cy = HUD_H + (*first + *last + 1) as f32 / 2.0 * TILE;
            draw_rectangle(W / 2.0 - 170.0, cy - 50.0, 340.0, 92.0, rgba(0, 0, 0, 200));
            text_centered("NO SIGNAL", W / 2.0, cy - 8.0, 34.0, rgb(230, 80, 60));
            text_centered(
                &format!("IT SERVER ROOM DOWN  {:.0}s", game.it_down().ceil()),
                W / 2.0,
                cy + 24.0,
                16.0,
                rgb(200, 200, 200),
            );
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
    if e.armed {
        // A rifle from Supply, held across the chest.
        let a = hip + vec2(-5.0 * s, -bh * 0.7);
        let b = a + vec2(15.0 * s, -6.0 * s);
        draw_line(a.x, a.y, b.x, b.y, 2.2 * s, shade(rgb(40, 36, 32), k));
        draw_line(
            a.x,
            a.y,
            a.x + 3.0 * s,
            a.y + 3.0 * s,
            2.0 * s,
            shade(rgb(110, 80, 50), k),
        );
    }
    if e.tools {
        // Sledgehammer: swung at barriers, carried on the shoulder otherwise.
        let grip = hip + vec2(4.0 * s, -bh * 0.8);
        let swing = if e.blocked {
            (time * 10.0 + e.id as f32).sin() * 1.2
        } else {
            -0.6
        };
        let dir = vec2(swing.sin(), -swing.cos());
        let head_p = grip + dir * 12.0 * s;
        draw_line(
            grip.x,
            grip.y,
            head_p.x,
            head_p.y,
            1.6 * s,
            shade(rgb(130, 100, 70), k),
        );
        draw_rectangle(
            head_p.x - 3.0 * s,
            head_p.y - 2.0 * s,
            6.0 * s,
            4.0 * s,
            shade(rgb(150, 150, 156), k),
        );
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
    it_room(game, time);
    supply_room(game, time);
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
    for b in game.barriers() {
        barrier_icon(px(b.pos), 1.0, b.hp / sc::stats::BARRIER_HP, 1.0);
    }
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

/// Whole-silo overview: stair, towers, rebels, and the visible window (`scroll` world px).
/// `r` is in screen pixels.
pub fn minimap(game: &Game, r: Rect, scroll: f32, time: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, rgba(8, 9, 12, 215));
    let map = game.map();
    let to = |v: sc::Vec2| vec2(r.x + v.x / COLS as f32 * r.w, r.y + v.y / ROWS as f32 * r.h);
    let row_y = |row: f32| r.y + row / ROWS as f32 * r.h;
    let mut counts = [0; SECTIONS.len()];
    for e in game.enemies().iter().filter(|e| game.is_visible(e.pos)) {
        counts[sc::map::section_of_row(e.pos.y.floor() as i32)] += 1;
    }
    for (a, b, front) in map.segments() {
        let (pa, pb) = (to(a), to(b));
        let c = if front { STAIR_FRONT } else { STAIR_BACK };
        draw_line(pa.x, pa.y, pb.x, pb.y, 1.5, with_alpha(c, 0.55));
    }
    for (i, &(name, first, _)) in SECTIONS.iter().enumerate() {
        let y = row_y(first as f32);
        if i > 0 {
            draw_line(r.x, y, r.x + r.w, y, 1.0, rgba(200, 160, 40, 160));
        }
        let short = match name {
            "UP TOP" => "TOP",
            "DOWN DEEP" => "DEEP",
            other => other,
        };
        let ly = if i == 0 { y + 22.0 } else { y + 11.0 };
        text_centered(short, r.x + r.w / 2.0, ly, 10.0, rgb(150, 136, 100));
        if counts[i] > 0 {
            // Rebel count for this section.
            let label = format!("{}", counts[i]);
            let w = measure(&label, 12.0) + 8.0;
            let by = ly + 4.0;
            draw_rectangle(r.x + r.w / 2.0 - w / 2.0, by, w, 14.0, rgb(150, 30, 24));
            text_centered(&label, r.x + r.w / 2.0, by + 11.0, 12.0, WHITE);
        }
    }
    let sp = to(sc::map::supply_center());
    let sp_col = if game.supply_joined() {
        rgb(230, 70, 50)
    } else {
        rgb(200, 160, 90)
    };
    draw_rectangle(sp.x - 4.0, sp.y - 3.0, 8.0, 6.0, sp_col);
    for b in game.barriers() {
        let p = to(b.pos);
        draw_rectangle(p.x - 4.0, p.y - 1.5, 8.0, 3.0, rgb(200, 160, 110));
    }
    let itp = to(sc::map::it_center());
    let it_col = if game.cameras_down() {
        rgb(230, 70, 50)
    } else {
        rgb(90, 160, 255)
    };
    draw_rectangle(itp.x - 5.0, itp.y - 3.0, 8.0, 6.0, it_col);
    let al = to(sc::vec2(AIRLOCK[0].0 as f32 + 1.0, 0.5));
    draw_rectangle(al.x - 6.0, al.y - 3.0, 12.0, 6.0, rgb(130, 130, 126));
    for t in game.towers() {
        let p = to(t.pos);
        draw_rectangle(p.x - 2.0, p.y - 2.0, 4.0, 4.0, tower_color(t.kind));
    }
    let (y0, y1) = (
        row_y(scroll / TILE),
        row_y(scroll / TILE + VIEW_ROWS as f32),
    );
    // Tiny climbing figures; the ones outside the visible window blink so they stand out.
    let blink = 0.55 + 0.45 * (time * 8.0).sin().abs();
    for e in game.enemies().iter().filter(|e| game.is_visible(e.pos)) {
        let p = to(e.pos) + vec2(e.lane * 6.0, 0.0);
        let s = if e.kind == EnemyKind::Leader {
            1.5
        } else {
            1.0
        };
        let a = if p.y < y0 || p.y > y1 { blink } else { 1.0 };
        let body = match e.kind {
            EnemyKind::Porter => rgb(240, 200, 130),
            EnemyKind::Mechanic => rgb(255, 140, 50),
            EnemyKind::Miner => rgb(230, 190, 40),
            EnemyKind::Leader => rgb(255, 50, 40),
        };
        draw_rectangle(
            p.x - 2.5 * s,
            p.y - 7.0 * s,
            5.0 * s,
            8.0 * s,
            rgba(0, 0, 0, 200),
        );
        draw_rectangle(
            p.x - 1.5 * s,
            p.y - 4.0 * s,
            3.0 * s,
            4.0 * s,
            with_alpha(body, a),
        );
        draw_circle(
            p.x,
            p.y - 5.5 * s,
            1.5 * s,
            with_alpha(rgb(240, 200, 170), a),
        );
    }
    // Whatever IT can't see is blacked out on the map too: a section without power, or the Mids
    // and Down Deep while the server room is down. Drawn over everything, towers included.
    for (i, &(_, first, last)) in SECTIONS.iter().enumerate() {
        let no_signal = game.cameras_down() && i > 0;
        let no_power = game
            .blackouts()
            .iter()
            .any(|b| b.first <= first && b.last >= last);
        if !(no_signal || no_power) {
            continue;
        }
        let (a, b) = (row_y(first as f32), row_y(last as f32 + 1.0));
        draw_rectangle(r.x, a, r.w, b - a, rgb(4, 4, 6));
        if no_signal {
            // Camera static.
            let seed = (time * 20.0) as u32;
            for k in 0..60u32 {
                let n =
                    (k.wrapping_mul(2_654_435_761) ^ seed.wrapping_mul(40_503)).rotate_left(k % 13);
                let x = r.x + (n % r.w as u32) as f32;
                let y = a + ((n >> 12) % (b - a) as u32) as f32;
                let g = (50 + (n >> 24) % 70) as u8;
                draw_rectangle(x, y, 2.0, 1.5, rgb(g, g, g));
            }
        }
        let (label, col) = if no_signal {
            ("NO", rgb(230, 80, 60))
        } else {
            ("OFF", rgb(170, 150, 100))
        };
        let mid = (a + b) / 2.0;
        text_centered(label, r.x + r.w / 2.0, mid - 2.0, 12.0, col);
        text_centered(
            if no_signal { "SIGNAL" } else { "POWER" },
            r.x + r.w / 2.0,
            mid + 11.0,
            9.0,
            col,
        );
    }
    draw_rectangle(r.x, y0, r.w, y1 - y0, rgba(255, 255, 255, 18));
    draw_rectangle_lines(r.x, y0, r.w, y1 - y0, 2.0, rgb(230, 200, 120));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, rgb(70, 64, 50));
}

/// A small up or down arrow button face. `alert` = rebels out of sight that way (pulses red).
pub fn arrow(r: Rect, up: bool, enabled: bool, hot: bool, alert: usize, time: f32) {
    let fill = if alert > 0 {
        let k = 0.5 + 0.5 * (time * 6.0).sin().abs();
        Color::new(0.45 * k + 0.1, 0.06, 0.05, 0.95)
    } else if hot && enabled {
        rgb(60, 54, 40)
    } else {
        rgba(8, 9, 12, 215)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, fill);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, rgb(110, 96, 60));
    let c = if enabled {
        rgb(230, 200, 120)
    } else {
        rgb(70, 66, 56)
    };
    let (cx, mut cy, s) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w.min(r.h) * 0.28);
    let d = if up { -1.0 } else { 1.0 };
    if alert > 0 {
        let label = format!("{alert}");
        let ty = if up { r.y + r.h - 5.0 } else { r.y + 14.0 };
        text_centered(&label, cx, ty, 13.0, WHITE);
        cy += if up { -6.0 } else { 6.0 };
    }
    draw_triangle(
        vec2(cx, cy + d * s),
        vec2(cx - s * 1.2, cy - d * s * 0.6),
        vec2(cx + s * 1.2, cy - d * s * 0.6),
        c,
    );
}
