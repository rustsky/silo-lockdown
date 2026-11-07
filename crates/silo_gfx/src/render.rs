//! Draws the silo cross-section, the spiral stair, towers and rebels.

use crate::light::Lighting;
use crate::ui::*;
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use silo_core as sc;
use silo_core::map::{
    AIRLOCK, COLS, FACILITIES, PILLAR_X, ROWS, SECTIONS, SHAFT_LEFT, SHAFT_RIGHT, VIEW_ROWS,
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
    /// Ceiling lamps on the living levels: (position, flicker phase, dead bulb).
    lamps: Vec<(Vec2, f32, bool)>,
    /// Furniture and machinery in the living levels, themed by section.
    props: Vec<PropInst>,
}

#[derive(Clone, Copy, PartialEq)]
enum Prop {
    // Up Top: offices and quarters.
    Desk,
    Bunk,
    Cabinet,
    Shelf,
    // Mids: farms, market, cafeteria.
    Planter,
    Stall,
    Table,
    Tank,
    Counter,
    Screen,
    GunRack,
    SchoolDesks,
    Chalkboard,
    Bed,
    MedCabinet,
    // Down Deep: machinery.
    Generator,
    Valves,
    Barrels,
    ToolRack,
    Cart,
}

struct PropInst {
    kind: Prop,
    x: f32,
    /// Top of the floor slab.
    floor: f32,
    seed: u32,
    flip: bool,
}

/// Visible world-pixel rows (top, bottom), for culling.
#[derive(Clone, Copy)]
pub struct Vis {
    pub y0: f32,
    pub y1: f32,
}

impl Vis {
    pub fn contains(&self, y: f32, margin: f32) -> bool {
        y >= self.y0 - margin && y <= self.y1 + margin
    }
}

/// Draw `tex` repeated over `r` (texture pixels map 1:1 to world pixels times `scale`).
fn tiled(tex: &Texture2D, r: Rect, tint: Color, scale: f32) {
    draw_texture_ex(
        tex,
        r.x,
        r.y,
        tint,
        DrawTextureParams {
            dest_size: Some(vec2(r.w, r.h)),
            source: Some(Rect::new(
                r.x * scale,
                r.y * scale,
                r.w * scale,
                r.h * scale,
            )),
            ..Default::default()
        },
    );
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
        let mut props = vec![];
        for r in 1..ROWS {
            for c in 0..COLS {
                if map.tile(c, r) != Tile::Floor {
                    continue;
                }
                let door = gen_range(0, 7) == 0;
                if !door && gen_range(0, 10) < 8 {
                    let choices = facility_props(sc::map::facility_at(c, r).unwrap_or(""));
                    props.push(PropInst {
                        kind: choices[gen_range(0, choices.len())],
                        x: c as f32 * TILE + TILE / 2.0 + gen_range(-6.0, 6.0),
                        floor: HUD_H + (r + 1) as f32 * TILE - 8.0,
                        seed: gen_range(0, u32::MAX),
                        flip: gen_range(0, 2) == 0,
                    });
                }
                if door || gen_range(0, 10) < 4 {
                    let x = c as f32 * TILE + gen_range(8.0, TILE - 26.0);
                    let y = HUD_H + r as f32 * TILE + gen_range(9.0, 13.0);
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
        let mut lamps = vec![];
        for r in 1..ROWS {
            for (lo, hi) in [(1.15, 2.6), (9.4, 10.85)] {
                let x = gen_range(lo, hi) * TILE;
                lamps.push((
                    vec2(x, HUD_H + r as f32 * TILE + 7.0),
                    gen_range(0.0, 10.0),
                    gen_range(0, 9) == 0,
                ));
            }
        }
        Self {
            lamps,
            props,
            specks,
            hills,
            ruins,
            dust,
            windows,
        }
    }
}

// ---------------------------------------------------------------- the silo

pub fn silo(game: &Game, decor: &Decor, time: f32, light: &mut Lighting, vis: Vis) {
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

    // The surface is in daylight: no lighting applied above ground.
    light
        .fullbright
        .push(Rect::new(0.0, top - 400.0, W, 400.0 + TILE * 0.82));
    // Daylight from the surface (through the haze).
    light.add(
        vec2(W / 2.0, top - 60.0),
        260.0,
        Color::new(0.95, 0.88, 0.75, 0.9),
    );
    light.add(
        vec2(W * 0.15, top - 20.0),
        200.0,
        Color::new(0.8, 0.72, 0.6, 0.8),
    );
    light.add(
        vec2(W * 0.85, top - 20.0),
        200.0,
        Color::new(0.8, 0.72, 0.6, 0.8),
    );

    // Earth around the silo.
    let earth_tint = rgb(78, 64, 50);
    tiled(
        &light.earth,
        Rect::new(0.0, top + TILE, TILE, WORLD_H - TILE),
        earth_tint,
        1.0,
    );
    tiled(
        &light.earth,
        Rect::new(W - TILE, top + TILE, TILE, WORLD_H - TILE),
        earth_tint,
        1.0,
    );
    for (p, r, c) in &decor.specks {
        draw_circle(p.x, p.y, *r, *c);
    }

    // Cylinder interior: poured concrete with curvature shading.
    let (ix0, ix1) = (TILE, W - TILE);
    tiled(
        &light.concrete,
        Rect::new(ix0, top + TILE, ix1 - ix0, WORLD_H - TILE),
        rgb(104, 106, 112),
        1.0,
    );
    let strips = 32;
    for i in 0..strips {
        let t = (i as f32 + 0.5) / strips as f32;
        let k = 0.35 + 0.65 * (PI * t).sin();
        let w = (ix1 - ix0) / strips as f32;
        draw_rectangle(
            ix0 + i as f32 * w,
            top + TILE,
            w + 0.5,
            WORLD_H - TILE,
            Color::new(0.0, 0.0, 0.0, 1.0 - k),
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
            tiled(
                &light.concrete,
                Rect::new(c as f32 * TILE, y - 8.0, TILE, 8.0),
                rgb(130, 132, 138),
                1.0,
            );
            draw_rectangle(c as f32 * TILE, y - 2.0, TILE, 2.0, rgba(0, 0, 0, 90));
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
    // Each facility has its own wall treatment.
    for r in 1..ROWS {
        let y = top + r as f32 * TILE;
        if !vis.contains(y + TILE / 2.0, TILE) {
            continue;
        }
        for c in 0..COLS {
            if map.tile(c, r) == Tile::Floor {
                let name = sc::map::facility_at(c, r).unwrap_or("");
                room_wall(
                    name,
                    Rect::new(c as f32 * TILE, y, TILE, TILE - 8.0),
                    &light.earth,
                    time,
                );
            }
        }
    }
    for (rect, phase, door) in &decor.windows {
        if *door {
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, rgb(34, 30, 28));
            draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.5, rgb(90, 84, 76));
        } else {
            let flicker =
                0.75 + 0.25 * ((time * 3.0 + phase).sin() * (time * 7.3 + phase * 2.0).sin()).abs();
            if vis.contains(rect.y, 60.0) {
                light.add(
                    rect.center(),
                    40.0,
                    Color::new(1.0, 0.72, 0.42, 0.16 * flicker),
                );
            }
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

    for p in &decor.props {
        if vis.contains(p.floor, 60.0) {
            // Soft shadow on the back wall, thrown away from this room's lamp.
            let row = ((p.floor + 8.0 - top) / TILE) as usize - 1;
            let side = if p.x < PILLAR_X * TILE { 0 } else { 1 };
            if let Some(&(lamp, _, _)) = decor.lamps.get((row - 1) * 2 + side) {
                let away = (p.x - lamp.x).signum();
                let lean = away * (8.0 + (p.x - lamp.x).abs() * 0.25).min(26.0);
                let (w, h) = (34.0, 30.0);
                let (bx, fy) = (p.x - w / 2.0, p.floor);
                for k in 0..3 {
                    let grow = k as f32 * 3.0;
                    draw_triangle(
                        vec2(bx - grow, fy),
                        vec2(bx + w + grow, fy),
                        vec2(bx + w + lean + grow, fy - h - grow),
                        rgba(0, 0, 0, 22),
                    );
                    draw_triangle(
                        vec2(bx - grow, fy),
                        vec2(bx + lean - grow, fy - h - grow),
                        vec2(bx + w + lean + grow, fy - h - grow),
                        rgba(0, 0, 0, 22),
                    );
                }
            }
            prop(p, time);
        }
    }
    // A name plate on the first floor of every facility.
    for &(row, _, left, right) in FACILITIES {
        let y = top + row as f32 * TILE + 3.0;
        if !vis.contains(y, TILE) {
            continue;
        }
        for (x, name, c) in [(TILE + 6.0, left, 1), (sx_right() + 6.0, right, 9)] {
            if map.tile(c, row) != Tile::Floor && map.tile(c + 1, row) != Tile::Floor {
                continue; // IT and Supply carry their own signs
            }
            let w = measure(name, 9.0) + 8.0;
            let x = if row == 1 && c == 1 {
                x + TILE * 0.9
            } else {
                x
            };
            draw_rectangle(x, y + 6.0, w, 11.0, rgb(28, 30, 34));
            draw_rectangle_lines(x, y + 6.0, w, 11.0, 1.0, facility_accent(name));
            text(
                name,
                x + 4.0,
                y + 14.5,
                9.0,
                shade(facility_accent(name), 1.2),
            );
        }
    }
    // Ceiling pipes along every level.
    for r in 1..ROWS {
        let y = top + r as f32 * TILE + 4.0;
        if !vis.contains(y, 60.0) {
            continue;
        }
        for (a, b) in [(TILE + 4.0, sx_left()), (sx_right(), W - TILE - 4.0)] {
            draw_line(a, y, b, y, 3.0, rgb(84, 86, 88));
            draw_line(a, y - 1.0, b, y - 1.0, 1.0, rgb(130, 132, 134));
            let mut bx = a + 10.0;
            while bx < b {
                draw_rectangle(bx, y - 4.0, 2.0, 3.0, rgb(60, 60, 62));
                bx += 30.0;
            }
        }
    }
    // Ceiling lamps on every level; a few bulbs are dead or dying.
    for &(p, phase, dead) in &decor.lamps {
        if !vis.contains(p.y, 80.0) {
            continue;
        }
        let row = ((p.y - top) / TILE) as i32;
        let dark = game.is_row_dark(row);
        let flick = if dead {
            let f = ((time * 13.0 + phase).sin() * (time * 7.7 + phase * 3.0).sin()).max(0.0);
            if f > 0.6 { 0.8 } else { 0.0 }
        } else {
            0.9 + 0.1 * (time * 2.0 + phase).sin()
        };
        // Industrial enamel shade on a drop rod.
        draw_line(p.x, p.y - 7.0, p.x, p.y - 2.0, 1.5, rgb(40, 40, 44));
        let shade_c = rgb(52, 72, 62);
        let (t, b) = (p.y - 3.0, p.y + 4.0);
        draw_triangle(
            vec2(p.x - 3.5, t),
            vec2(p.x + 3.5, t),
            vec2(p.x + 11.0, b),
            shade_c,
        );
        draw_triangle(
            vec2(p.x - 3.5, t),
            vec2(p.x - 11.0, b),
            vec2(p.x + 11.0, b),
            shade_c,
        );
        draw_line(p.x - 3.5, t, p.x - 11.0, b, 1.0, shade(shade_c, 1.5));
        draw_line(p.x - 11.0, b, p.x + 11.0, b, 1.2, rgb(150, 150, 140));
        draw_rectangle(p.x - 2.0, t - 1.5, 4.0, 2.0, rgb(70, 72, 70));
        let bulb = if flick > 0.0 && !dark {
            Color::new(1.0, 0.92, 0.7, 1.0)
        } else {
            rgb(70, 66, 60)
        };
        draw_circle(p.x, p.y + 4.5, 2.6, bulb);
        if flick > 0.0 && !dark {
            // A cone of warm light down to the floor, plus a little spill on the ceiling.
            light.add_cone(
                p + vec2(0.0, 3.0),
                TILE - 14.0,
                92.0,
                Color::new(1.0, 0.8, 0.52, 0.95 * flick),
            );
            light.add(
                p + vec2(0.0, 4.0),
                30.0,
                Color::new(1.0, 0.8, 0.52, 0.35 * flick),
            );
        }
    }

    // The open stair shaft: a dark back wall with pipes and cable runs.
    let (sx0, sx1) = (SHAFT_LEFT.floor() * TILE, SHAFT_RIGHT.ceil() * TILE);
    tiled(
        &light.concrete,
        Rect::new(sx0, top + TILE, sx1 - sx0, WORLD_H - TILE),
        rgb(36, 38, 46),
        0.7,
    );
    for i in 0..16 {
        let t = (i as f32 + 0.5) / 16.0;
        let k = 0.45 + 0.55 * (PI * t).sin();
        let w = (sx1 - sx0) / 16.0;
        draw_rectangle(
            sx0 + i as f32 * w,
            top + TILE,
            w + 0.5,
            WORLD_H - TILE,
            Color::new(0.0, 0.0, 0.0, 1.0 - k),
        );
    }
    let (vy0, vy1) = (vis.y0.max(top + TILE), vis.y1.min(bottom));
    for (px_, pw, c) in [
        (sx0 + 18.0, 10.0, rgb(70, 74, 80)),
        (sx0 + 34.0, 6.0, rgb(92, 70, 52)),
        (sx1 - 26.0, 12.0, rgb(64, 70, 66)),
    ] {
        // Shaded cylinder: dark edge, body, highlight.
        draw_rectangle(px_ - pw / 2.0, vy0, pw, vy1 - vy0, shade(c, 0.6));
        draw_rectangle(px_ - pw / 2.0 + 1.5, vy0, pw - 4.0, vy1 - vy0, c);
        draw_rectangle(px_ - pw / 2.0 + 2.5, vy0, 1.5, vy1 - vy0, shade(c, 1.5));
        let mut y = ((vy0 - top) / TILE).floor() * TILE + top + 20.0;
        while y < vy1 {
            draw_rectangle(px_ - pw / 2.0 - 3.0, y, pw + 6.0, 5.0, rgb(44, 46, 50));
            y += TILE;
        }
    }
    // Dust drifting in the stairwell air; the lamps pick it out.
    for i in 0..90u32 {
        let h = |k: u32| {
            ((i.wrapping_mul(2_654_435_761) ^ k.wrapping_mul(97_531)) % 10_000) as f32 / 10_000.0
        };
        let span = vy1 - vy0 + 40.0;
        let x = sx0 + h(1) * (sx1 - sx0) + (time * 0.4 + h(4) * 6.0).sin() * 6.0;
        let y = vy0 - 20.0 + (h(2) * span + time * (4.0 + 8.0 * h(3))).rem_euclid(span);
        draw_circle(x, y, 0.8 + h(5), rgba(230, 220, 200, 70));
    }
    // Sagging cable between brackets.
    let mut y = ((vy0 - top) / TILE).floor() * TILE + top;
    while y < vy1 + TILE {
        let (a, b) = (vec2(sx1 - 44.0, y), vec2(sx1 - 44.0, y + TILE));
        for k in 0..6 {
            let t0 = k as f32 / 6.0;
            let t1 = (k + 1) as f32 / 6.0;
            let sag = |t: f32| (t * PI).sin() * 6.0;
            let p0 = a.lerp(b, t0) + vec2(sag(t0), 0.0);
            let p1 = a.lerp(b, t1) + vec2(sag(t1), 0.0);
            draw_line(p0.x, p0.y, p1.x, p1.y, 2.0, rgb(24, 24, 26));
        }
        y += TILE;
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
        // Section plate painted on the shaft wall, beside the central column.
        let row = first.max(2);
        let sy = top + row as f32 * TILE + 8.0;
        let label = format!("{name}  {}-{}", level_of_row(first), level_of_row(last) + 3);
        let w = measure(&label, 12.0) + 14.0;
        let sx = PILLAR_X * TILE + 22.0;
        draw_rectangle(sx, sy, w, 18.0, rgb(30, 34, 40));
        draw_rectangle_lines(sx, sy, w, 18.0, 1.5, rgb(200, 160, 60));
        text(&label, sx + 7.0, sy + 13.5, 12.0, rgb(230, 196, 100));
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
}

// ---------------------------------------------------------------- furniture & machinery

fn sx_left() -> f32 {
    SHAFT_LEFT.floor() * TILE
}

fn sx_right() -> f32 {
    SHAFT_RIGHT.ceil() * TILE
}

/// Furniture that belongs in a facility.
fn facility_props(name: &str) -> &'static [Prop] {
    match name {
        "CAFETERIA" => &[Prop::Table, Prop::Table, Prop::Counter, Prop::Screen],
        "MAYOR" | "IT OFFICES" => &[Prop::Desk, Prop::Cabinet, Prop::Shelf],
        "SHERIFF" => &[Prop::Desk, Prop::GunRack, Prop::Cabinet],
        "QUARTERS" => &[Prop::Bunk, Prop::Bunk, Prop::Shelf, Prop::Table],
        "SCHOOL" => &[Prop::SchoolDesks, Prop::Chalkboard, Prop::SchoolDesks],
        "HOSPITAL" | "CLINIC" => &[Prop::Bed, Prop::Bed, Prop::MedCabinet],
        "MARKET" => &[Prop::Stall, Prop::Stall, Prop::Barrels],
        "FARMS" => &[Prop::Planter],
        "WATER WORKS" | "PUMPS" => &[Prop::Tank, Prop::Valves],
        "WORKSHOP" => &[Prop::ToolRack, Prop::Barrels, Prop::ToolRack],
        "STORES" => &[Prop::Barrels, Prop::Shelf],
        "POWER PLANT" => &[Prop::Generator],
        "MINE" => &[Prop::Cart, Prop::Cart, Prop::Barrels],
        "MECHANICAL" => &[Prop::Generator, Prop::Valves],
        _ => &[Prop::Shelf],
    }
}

/// Signage colour per facility.
fn facility_accent(name: &str) -> Color {
    match name {
        "HOSPITAL" | "CLINIC" => rgb(220, 70, 60),
        "FARMS" => rgb(120, 200, 100),
        "SCHOOL" => rgb(230, 190, 90),
        "CAFETERIA" | "MARKET" => rgb(230, 150, 80),
        "WATER WORKS" | "PUMPS" => rgb(90, 170, 230),
        "POWER PLANT" | "MECHANICAL" | "MINE" | "WORKSHOP" | "STORES" => rgb(230, 190, 40),
        "SHERIFF" => rgb(200, 200, 210),
        _ => rgb(180, 170, 150),
    }
}

/// Wall treatment for one tile of a facility (drawn over the bare concrete).
fn room_wall(name: &str, r: Rect, rock: &Texture2D, time: f32) {
    match name {
        "HOSPITAL" | "CLINIC" => {
            draw_rectangle(r.x, r.y, r.w, r.h, rgba(220, 230, 234, 150));
            let mut y = r.y + 6.0;
            while y < r.y + r.h {
                draw_line(r.x, y, r.x + r.w, y, 0.6, rgba(150, 170, 176, 120));
                y += 7.0;
            }
            draw_rectangle(r.x, r.y + r.h - 14.0, r.w, 2.0, rgba(90, 150, 160, 160));
        }
        "CAFETERIA" => {
            draw_rectangle(r.x, r.y + r.h - 16.0, r.w, 16.0, rgb(108, 78, 52));
            draw_line(
                r.x,
                r.y + r.h - 16.0,
                r.x + r.w,
                r.y + r.h - 16.0,
                1.5,
                rgb(150, 110, 70),
            );
            let mut x = r.x + 4.0;
            while x < r.x + r.w {
                draw_line(x, r.y + r.h - 15.0, x, r.y + r.h, 0.6, rgb(84, 60, 40));
                x += 8.0;
            }
        }
        "SCHOOL" => {
            draw_rectangle(r.x, r.y, r.w, r.h, rgba(200, 170, 110, 60));
            draw_rectangle(r.x, r.y + r.h - 12.0, r.w, 12.0, rgba(90, 120, 90, 110));
        }
        "MAYOR" | "SHERIFF" | "IT OFFICES" => {
            let mut x = r.x + 3.0;
            while x < r.x + r.w {
                draw_rectangle(x, r.y, 3.0, r.h, rgba(120, 90, 70, 50));
                x += 9.0;
            }
        }
        "FARMS" => {
            draw_rectangle(r.x, r.y, r.w, r.h, rgba(90, 160, 90, 40));
            // Pink grow-light tubes under the ceiling.
            draw_rectangle(r.x + 4.0, r.y + 8.0, r.w - 8.0, 2.0, rgb(240, 130, 210));
            draw_rectangle(r.x, r.y + r.h - 4.0, r.w, 4.0, rgb(70, 50, 34)); // soil spill
        }
        "MARKET" => {
            // Bunting along the ceiling.
            for i in 0..7 {
                let x = r.x + i as f32 * 8.0;
                let c = [rgb(200, 60, 50), rgb(230, 200, 70), rgb(70, 130, 200)][i % 3];
                draw_triangle(
                    vec2(x, r.y + 8.0),
                    vec2(x + 7.0, r.y + 8.0),
                    vec2(x + 3.5, r.y + 13.0),
                    c,
                );
            }
            draw_line(r.x, r.y + 8.0, r.x + r.w, r.y + 8.0, 0.8, rgb(60, 50, 40));
        }
        "WATER WORKS" | "PUMPS" => {
            draw_rectangle(r.x, r.y, r.w, r.h, rgba(60, 110, 150, 45));
            draw_line(
                r.x,
                r.y + 14.0,
                r.x + r.w,
                r.y + 14.0,
                4.0,
                rgb(60, 100, 140),
            );
            draw_line(
                r.x,
                r.y + 13.0,
                r.x + r.w,
                r.y + 13.0,
                1.0,
                rgb(110, 160, 200),
            );
        }
        "POWER PLANT" | "MECHANICAL" | "WORKSHOP" | "STORES" => {
            draw_rectangle(r.x, r.y, r.w, r.h, rgba(20, 18, 14, 70));
            let mut x = r.x;
            let mut k = 0;
            while x < r.x + r.w {
                let c = if k % 2 == 0 {
                    rgb(200, 160, 30)
                } else {
                    rgb(30, 30, 30)
                };
                draw_rectangle(x, r.y + r.h - 3.0, 6.0, 3.0, c);
                x += 6.0;
                k += 1;
            }
            if name == "POWER PLANT" {
                // Buzzing cable trunk.
                draw_line(r.x, r.y + 16.0, r.x + r.w, r.y + 16.0, 3.0, rgb(30, 30, 32));
                if (time * 20.0 + r.x).sin() > 0.95 {
                    draw_circle(r.x + r.w / 2.0, r.y + 16.0, 2.0, rgb(180, 220, 255));
                }
            }
        }
        "MINE" => {
            // Raw rock with timber supports.
            draw_texture_ex(
                rock,
                r.x,
                r.y,
                rgb(110, 92, 74),
                DrawTextureParams {
                    dest_size: Some(vec2(r.w, r.h)),
                    source: Some(Rect::new(r.x, r.y, r.w, r.h)),
                    ..Default::default()
                },
            );
            draw_rectangle(r.x + 4.0, r.y, 5.0, r.h, rgb(110, 80, 50));
            draw_rectangle(r.x, r.y + 4.0, r.w, 5.0, rgb(120, 88, 56));
        }
        _ => {}
    }
}

/// Axis-aligned box with a lit top edge and a shaded right side, `(x, y)` = top-left.
fn boxy(x: f32, y: f32, w: f32, h: f32, c: Color) {
    draw_rectangle(x, y, w, h, c);
    draw_rectangle(x + w - 2.0, y, 2.0, h, shade(c, 0.7));
    draw_line(x, y + 0.5, x + w, y + 0.5, 1.0, shade(c, 1.35));
    draw_rectangle(x, y + h - 1.0, w, 1.0, shade(c, 0.6));
}

fn prop(p: &PropInst, time: f32) {
    let f = if p.flip { -1.0 } else { 1.0 };
    let x = |dx: f32| p.x + dx * f;
    // Left edge of a w-wide thing whose near edge is at offset dx.
    let lx = |dx: f32, w: f32| if p.flip { p.x - dx - w } else { p.x + dx };
    let fl = p.floor;
    let v = |k: u32| ((p.seed >> (k * 3)) & 7) as f32 / 7.0;
    draw_ellipse(p.x, fl, 22.0, 2.5, 0.0, rgba(0, 0, 0, 70));
    match p.kind {
        Prop::Desk => {
            // Desk, CRT monitor with a green terminal, swivel chair.
            boxy(lx(-20.0, 30.0), fl - 15.0, 30.0, 3.0, rgb(110, 84, 58));
            draw_rectangle(lx(-18.0, 3.0), fl - 12.0, 3.0, 12.0, rgb(70, 54, 40));
            boxy(lx(4.0, 6.0), fl - 12.0, 6.0, 12.0, rgb(96, 74, 52));
            boxy(lx(-14.0, 14.0), fl - 28.0, 14.0, 12.0, rgb(170, 166, 150));
            let glow = 0.75 + 0.25 * (time * 3.0 + v(1) * 9.0).sin();
            draw_rectangle(
                lx(-12.0, 9.0),
                fl - 26.0,
                9.0,
                7.0,
                Color::new(0.15, 0.55 * glow, 0.25, 1.0),
            );
            for i in 0..3 {
                draw_line(
                    lx(-11.0, 6.0),
                    fl - 24.5 + i as f32 * 2.0,
                    lx(-11.0, 6.0) + 3.0 + v(i) * 4.0,
                    fl - 24.5 + i as f32 * 2.0,
                    0.8,
                    rgb(150, 255, 160),
                );
            }
            draw_rectangle(x(-7.0) - 1.0, fl - 16.0, 3.0, 1.5, rgb(60, 60, 60));
            // Chair.
            let cx = x(16.0);
            draw_line(cx, fl - 1.0, cx, fl - 9.0, 1.5, rgb(50, 50, 54));
            draw_rectangle(cx - 5.0, fl - 11.0, 10.0, 2.5, rgb(60, 70, 96));
            draw_line(
                cx + 4.0 * f,
                fl - 11.0,
                cx + 5.0 * f,
                fl - 20.0,
                2.0,
                rgb(60, 70, 96),
            );
        }
        Prop::Bunk => {
            // Two-tier bunk with blankets and a pillow.
            let (bx, w) = (lx(-22.0, 40.0), 40.0);
            for y in [fl - 7.0, fl - 20.0] {
                boxy(bx, y - 3.0, w, 4.0, rgb(150, 146, 136));
                draw_rectangle(
                    bx + 2.0,
                    y - 6.0,
                    w - 4.0,
                    3.5,
                    rgb(90 + (v(2) * 60.0) as u8, 70, 60),
                );
                draw_rectangle(lx(-20.0, 7.0), y - 7.0, 7.0, 3.0, rgb(220, 216, 200));
            }
            for px_ in [bx, bx + w - 2.0] {
                draw_rectangle(px_, fl - 27.0, 2.0, 27.0, rgb(80, 82, 86));
            }
        }
        Prop::Cabinet => {
            boxy(lx(-10.0, 16.0), fl - 30.0, 16.0, 30.0, rgb(108, 112, 104));
            for i in 0..3 {
                let y = fl - 28.0 + i as f32 * 9.5;
                draw_rectangle_lines(lx(-9.0, 13.0), y, 13.0, 8.5, 1.0, rgb(70, 72, 68));
                draw_rectangle(lx(-5.0, 5.0), y + 3.0, 5.0, 1.5, rgb(180, 180, 170));
            }
            // A potted plant on top.
            draw_rectangle(lx(-6.0, 6.0), fl - 35.0, 6.0, 5.0, rgb(150, 90, 60));
            draw_circle(x(-3.0), fl - 38.0, 4.0, rgb(70, 120, 60));
        }
        Prop::Shelf => {
            let (bx, w) = (lx(-18.0, 34.0), 34.0);
            draw_rectangle(bx, fl - 32.0, w, 32.0, rgb(70, 52, 38));
            for i in 0..3 {
                let y = fl - 31.0 + i as f32 * 10.5;
                draw_rectangle(bx, y + 8.0, w, 2.0, rgb(110, 82, 58));
                let mut bxx = bx + 2.0;
                let mut k = i as u32;
                while bxx < bx + w - 4.0 {
                    let bw = 2.0 + ((p.seed >> (k % 24)) & 3) as f32;
                    let hgt = 5.0 + ((p.seed >> ((k + 5) % 24)) & 3) as f32;
                    let hue = [
                        rgb(150, 50, 40),
                        rgb(50, 80, 130),
                        rgb(60, 110, 60),
                        rgb(180, 160, 90),
                    ][(k % 4) as usize];
                    draw_rectangle(bxx, y + 8.0 - hgt, bw, hgt, hue);
                    bxx += bw + 0.6;
                    k += 1;
                }
            }
        }
        Prop::Planter => {
            // Hydroponic rack: two trays of plants under a grow-light bar.
            let (bx, w) = (lx(-22.0, 44.0), 44.0);
            draw_rectangle(bx, fl - 34.0, w, 2.5, rgb(80, 80, 84));
            draw_rectangle(bx + 3.0, fl - 32.0, w - 6.0, 1.5, rgb(255, 150, 220));
            for (i, y) in [fl - 4.0, fl - 18.0].into_iter().enumerate() {
                boxy(bx, y - 3.0, w, 4.0, rgb(90, 96, 100));
                let mut px_ = bx + 3.0;
                let mut k = i as u32 * 7;
                while px_ < bx + w - 2.0 {
                    let hgt = 5.0 + ((p.seed >> (k % 27)) & 7) as f32;
                    let g = rgb(60 + (k * 13 % 40) as u8, 140 + (k * 7 % 50) as u8, 60);
                    draw_line(px_, y - 3.0, px_, y - 3.0 - hgt, 1.2, shade(g, 0.7));
                    draw_circle(px_, y - 3.0 - hgt, 2.4, g);
                    draw_circle(px_ + 1.5, y - 1.5 - hgt * 0.6, 1.8, shade(g, 1.15));
                    px_ += 5.0;
                    k += 1;
                }
            }
            for px_ in [bx + 1.0, bx + w - 2.0] {
                draw_rectangle(px_, fl - 34.0, 1.5, 34.0, rgb(70, 72, 76));
            }
        }
        Prop::Stall => {
            // Market stall: striped awning, counter, crates of produce.
            let (bx, w) = (lx(-22.0, 44.0), 44.0);
            for i in 0..6 {
                let c = if i % 2 == 0 {
                    rgb(170, 60, 50)
                } else {
                    rgb(220, 210, 180)
                };
                draw_triangle(
                    vec2(bx + i as f32 * 7.3, fl - 34.0),
                    vec2(bx + (i + 1) as f32 * 7.3, fl - 34.0),
                    vec2(bx + (i as f32 + 0.5) * 7.3, fl - 28.0),
                    c,
                );
                draw_rectangle(bx + i as f32 * 7.3, fl - 38.0, 7.4, 4.0, c);
            }
            for px_ in [bx + 1.0, bx + w - 2.0] {
                draw_rectangle(px_, fl - 34.0, 1.5, 34.0, rgb(100, 80, 60));
            }
            boxy(bx + 3.0, fl - 12.0, w - 6.0, 12.0, rgb(120, 92, 60));
            for i in 0..5 {
                let c = [
                    rgb(230, 120, 40),
                    rgb(200, 40, 40),
                    rgb(230, 210, 60),
                    rgb(110, 170, 60),
                    rgb(230, 120, 40),
                ][i];
                draw_circle(bx + 8.0 + i as f32 * 7.0, fl - 14.0, 2.6, c);
            }
        }
        Prop::Table => {
            // Cafeteria table with stools and trays.
            let (bx, w) = (lx(-20.0, 38.0), 38.0);
            boxy(bx, fl - 13.0, w, 3.0, rgb(150, 150, 142));
            for lx_ in [bx + 4.0, bx + w - 6.0] {
                draw_rectangle(lx_, fl - 10.0, 2.0, 10.0, rgb(80, 80, 84));
            }
            draw_rectangle(bx + 6.0, fl - 15.0, 9.0, 2.0, rgb(180, 140, 80));
            draw_circle(bx + 10.0, fl - 16.0, 2.0, rgb(230, 220, 190));
            for sx in [bx - 4.0, bx + w + 1.0] {
                draw_rectangle(sx, fl - 8.0, 5.0, 2.0, rgb(120, 60, 50));
                draw_rectangle(sx + 2.0, fl - 6.0, 1.2, 6.0, rgb(70, 70, 74));
            }
        }
        Prop::Tank => {
            // Water tank with level gauge and pipe.
            let (bx, w) = (lx(-14.0, 26.0), 26.0);
            draw_rectangle(bx, fl - 30.0, w, 30.0, rgb(70, 100, 120));
            for i in 0..5 {
                let t = (i as f32 + 0.5) / 5.0;
                let k = 0.6 + 0.4 * (PI * t).sin();
                draw_rectangle(
                    bx + t * w - w / 10.0,
                    fl - 30.0,
                    w / 5.0,
                    30.0,
                    Color::new(0.27 * k, 0.4 * k, 0.48 * k, 1.0),
                );
            }
            draw_ellipse(
                bx + w / 2.0,
                fl - 30.0,
                w / 2.0,
                3.0,
                0.0,
                rgb(110, 140, 160),
            );
            for y in [fl - 22.0, fl - 10.0] {
                draw_line(bx, y, bx + w, y, 1.5, rgb(50, 70, 84));
            }
            draw_rectangle(x(10.0) - 1.0, fl - 26.0, 3.0, 16.0, rgb(30, 40, 46));
            draw_rectangle(x(10.0) - 1.0, fl - 18.0, 3.0, 8.0, rgb(90, 160, 220));
            draw_line(x(-14.0), fl - 6.0, x(-24.0), fl - 6.0, 3.0, rgb(90, 92, 96));
        }
        Prop::Counter => {
            // Serving counter with a steaming pot.
            let (bx, w) = (lx(-22.0, 44.0), 44.0);
            boxy(bx, fl - 15.0, w, 15.0, rgb(150, 150, 146));
            draw_rectangle(bx, fl - 16.0, w, 2.0, rgb(190, 190, 186));
            draw_rectangle(lx(-14.0, 12.0), fl - 23.0, 12.0, 7.0, rgb(90, 92, 96));
            for i in 0..3 {
                let t = (time * 0.7 + i as f32 * 0.33).fract();
                draw_circle(
                    x(-8.0) + (t * 6.0).sin() * 2.0,
                    fl - 25.0 - t * 12.0,
                    1.5 + t * 2.0,
                    rgba(230, 230, 230, ((1.0 - t) * 90.0) as u8),
                );
            }
            for i in 0..3 {
                draw_rectangle(
                    x(4.0 + i as f32 * 6.0) - 2.0,
                    fl - 19.0,
                    4.0,
                    3.0,
                    rgb(200, 190, 160),
                );
            }
        }
        Prop::Screen => {
            // The cafeteria wallscreen: the grey outside, the dead hills.
            let r = Rect::new(p.x - 24.0, fl - 40.0, 48.0, 22.0);
            draw_rectangle(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, rgb(30, 30, 32));
            for i in 0..6 {
                let t = i as f32 / 6.0;
                draw_rectangle(
                    r.x,
                    r.y + t * r.h,
                    r.w,
                    r.h / 6.0 + 0.5,
                    Color::new(0.45 + 0.15 * t, 0.43 + 0.12 * t, 0.38 + 0.08 * t, 1.0),
                );
            }
            draw_triangle(
                vec2(r.x, r.y + r.h),
                vec2(r.x + 18.0, r.y + 12.0),
                vec2(r.x + 34.0, r.y + r.h),
                rgb(90, 80, 66),
            );
            draw_triangle(
                vec2(r.x + 22.0, r.y + r.h),
                vec2(r.x + 38.0, r.y + 9.0),
                vec2(r.x + r.w, r.y + r.h),
                rgb(76, 68, 58),
            );
            for k in 0..3 {
                let y = r.y + ((time * 6.0 + k as f32 * 7.0) % r.h);
                draw_line(r.x, y, r.x + r.w, y, 0.6, rgba(255, 255, 255, 25));
            }
            draw_line(p.x, r.y + r.h + 2.0, p.x, fl, 1.5, rgb(50, 50, 54));
        }
        Prop::GunRack => {
            let (bx, w) = (lx(-16.0, 30.0), 30.0);
            draw_rectangle(bx, fl - 34.0, w, 30.0, rgb(70, 54, 40));
            draw_rectangle_lines(bx, fl - 34.0, w, 30.0, 1.0, rgb(40, 30, 22));
            for i in 0..4 {
                let gx = bx + 5.0 + i as f32 * 7.0;
                draw_line(gx, fl - 8.0, gx + 1.5, fl - 31.0, 1.6, rgb(30, 30, 32));
                draw_line(gx, fl - 8.0, gx - 1.0, fl - 14.0, 2.4, rgb(110, 76, 46));
            }
            // Sheriff's star.
            draw_poly(x(18.0), fl - 30.0, 5, 3.5, 0.0, rgb(220, 190, 80));
        }
        Prop::SchoolDesks => {
            // Two small desks with stools, satchels.
            for i in 0..2 {
                let dx = -18.0 + i as f32 * 22.0;
                boxy(lx(dx, 14.0), fl - 11.0, 14.0, 2.5, rgb(170, 130, 80));
                draw_rectangle(lx(dx + 1.0, 1.5), fl - 9.0, 1.5, 9.0, rgb(90, 70, 50));
                draw_rectangle(lx(dx + 11.0, 1.5), fl - 9.0, 1.5, 9.0, rgb(90, 70, 50));
                draw_rectangle(lx(dx + 4.0, 6.0), fl - 13.0, 6.0, 1.5, rgb(240, 236, 220)); // paper
                draw_rectangle(lx(dx + 15.0, 4.0), fl - 5.0, 4.0, 5.0, rgb(150, 60, 50));
            }
        }
        Prop::Chalkboard => {
            let r = Rect::new(p.x - 22.0, fl - 38.0, 44.0, 20.0);
            draw_rectangle(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, rgb(120, 86, 54));
            draw_rectangle(r.x, r.y, r.w, r.h, rgb(40, 70, 52));
            for (i, l) in ["2+2=4", "SILO 18"].iter().enumerate() {
                text(
                    l,
                    r.x + 4.0,
                    r.y + 8.0 + i as f32 * 8.0,
                    8.0,
                    rgb(220, 226, 220),
                );
            }
            draw_rectangle(r.x, r.y + r.h + 1.0, r.w, 2.0, rgb(150, 110, 70));
            // Teacher's desk.
            boxy(lx(6.0, 16.0), fl - 10.0, 16.0, 10.0, rgb(140, 104, 66));
        }
        Prop::Bed => {
            // Hospital bed, IV stand, half-drawn curtain.
            let (bx, w) = (lx(-20.0, 34.0), 34.0);
            draw_rectangle(bx, fl - 10.0, w, 3.0, rgb(180, 184, 190));
            draw_rectangle(bx + 1.0, fl - 13.0, w - 2.0, 3.5, rgb(236, 240, 244));
            draw_rectangle(lx(-20.0, 8.0), fl - 15.0, 8.0, 3.0, rgb(250, 250, 250));
            for lx_ in [bx + 1.0, bx + w - 2.0] {
                draw_rectangle(lx_, fl - 7.0, 1.5, 7.0, rgb(140, 144, 150));
                draw_circle(lx_ + 0.7, fl - 0.5, 1.2, rgb(40, 40, 40));
            }
            let ivx = x(18.0);
            draw_line(ivx, fl, ivx, fl - 30.0, 1.0, rgb(160, 164, 170));
            draw_rectangle(ivx - 2.5, fl - 30.0, 5.0, 7.0, rgba(200, 230, 255, 200));
            let cx = lx(-26.0, 10.0);
            draw_rectangle(cx, fl - 36.0, 10.0, 30.0, rgba(130, 180, 170, 200));
            for k in 0..3 {
                draw_line(
                    cx + 2.0 + k as f32 * 3.0,
                    fl - 36.0,
                    cx + 2.0 + k as f32 * 3.0,
                    fl - 6.0,
                    0.7,
                    rgba(90, 140, 130, 200),
                );
            }
        }
        Prop::MedCabinet => {
            boxy(lx(-10.0, 22.0), fl - 30.0, 22.0, 30.0, rgb(226, 230, 234));
            draw_rectangle(
                lx(-10.0, 22.0) + 2.0,
                fl - 28.0,
                18.0,
                12.0,
                rgba(160, 200, 220, 160),
            );
            for i in 0..4 {
                draw_rectangle(
                    lx(-10.0, 22.0) + 4.0 + i as f32 * 4.0,
                    fl - 24.0,
                    2.5,
                    6.0,
                    [
                        rgb(200, 60, 50),
                        rgb(240, 200, 80),
                        rgb(90, 160, 90),
                        rgb(100, 120, 200),
                    ][i],
                );
            }
            // Red cross.
            let c = vec2(x(1.0), fl - 8.0);
            draw_rectangle(c.x - 4.0, c.y - 1.5, 8.0, 3.0, rgb(210, 40, 40));
            draw_rectangle(c.x - 1.5, c.y - 4.0, 3.0, 8.0, rgb(210, 40, 40));
        }
        Prop::Generator => {
            // Diesel generator: housing, vents, gauges, exhaust pipe.
            let (bx, w) = (lx(-22.0, 42.0), 42.0);
            boxy(bx, fl - 22.0, w, 22.0, rgb(150, 120, 50));
            for i in 0..5 {
                draw_line(
                    bx + 4.0,
                    fl - 18.0 + i as f32 * 3.0,
                    bx + 18.0,
                    fl - 18.0 + i as f32 * 3.0,
                    1.2,
                    rgb(70, 56, 24),
                );
            }
            for (i, gx) in [26.0, 34.0].iter().enumerate() {
                let c = vec2(bx + gx, fl - 14.0);
                draw_circle(c.x, c.y, 3.5, rgb(230, 226, 210));
                draw_circle_lines(c.x, c.y, 3.5, 1.0, rgb(40, 40, 40));
                let a = time * (1.3 + i as f32) + v(i as u32) * 6.0;
                draw_line(
                    c.x,
                    c.y,
                    c.x + a.sin() * 2.8,
                    c.y - a.cos().abs() * 2.8,
                    0.8,
                    rgb(200, 40, 30),
                );
            }
            draw_rectangle(
                bx + 26.0,
                fl - 7.0,
                3.0,
                3.0,
                if (time * 2.0).sin() > 0.0 {
                    rgb(90, 220, 90)
                } else {
                    rgb(30, 70, 30)
                },
            );
            draw_rectangle(lx(14.0, 4.0), fl - 34.0, 4.0, 12.0, rgb(60, 60, 60));
            draw_rectangle(lx(12.0, 8.0), fl - 35.0, 8.0, 2.0, rgb(80, 80, 80));
        }
        Prop::Valves => {
            // Pipe manifold with valve wheels and a pressure gauge.
            let y0 = fl - 26.0;
            for (i, yy) in [y0, y0 + 10.0].iter().enumerate() {
                draw_rectangle(
                    p.x - 24.0,
                    *yy,
                    48.0,
                    5.0,
                    shade(rgb(110, 90, 70), 0.8 + 0.2 * i as f32),
                );
                draw_line(
                    p.x - 24.0,
                    *yy + 1.0,
                    p.x + 24.0,
                    *yy + 1.0,
                    1.0,
                    rgb(160, 136, 110),
                );
            }
            for vx in [-12.0, 10.0] {
                let c = vec2(x(vx), y0 + 2.5);
                draw_circle_lines(c.x, c.y, 4.5, 1.6, rgb(190, 40, 30));
                draw_line(c.x - 4.5, c.y, c.x + 4.5, c.y, 1.0, rgb(190, 40, 30));
                draw_line(c.x, c.y - 4.5, c.x, c.y + 4.5, 1.0, rgb(190, 40, 30));
            }
            draw_rectangle(x(0.0) - 2.0, y0 + 5.0, 4.0, 21.0, rgb(100, 82, 64));
            let g = vec2(x(18.0), y0 + 18.0);
            draw_circle(g.x, g.y, 4.0, rgb(230, 226, 210));
            draw_circle_lines(g.x, g.y, 4.0, 1.0, rgb(40, 40, 40));
            let a = (time * 0.7 + v(3) * 5.0).sin();
            draw_line(g.x, g.y, g.x + a * 3.0, g.y - 2.5, 0.8, rgb(200, 40, 30));
        }
        Prop::Barrels => {
            for (i, bxo) in [-18.0, -4.0, 10.0].iter().enumerate() {
                if i == 2 && v(4) < 0.4 {
                    continue;
                }
                let c = [rgb(60, 90, 140), rgb(150, 60, 40), rgb(70, 110, 70)]
                    [(p.seed as usize + i) % 3];
                let bx = lx(*bxo, 12.0);
                draw_rectangle(bx, fl - 17.0, 12.0, 17.0, c);
                draw_rectangle(bx + 8.0, fl - 17.0, 4.0, 17.0, shade(c, 0.7));
                draw_rectangle(bx + 1.0, fl - 17.0, 2.0, 17.0, shade(c, 1.3));
                for y in [fl - 13.0, fl - 5.0] {
                    draw_line(bx, y, bx + 12.0, y, 1.2, shade(c, 0.5));
                }
                draw_ellipse(bx + 6.0, fl - 17.0, 6.0, 1.6, 0.0, shade(c, 1.2));
            }
            // Hazard sign.
            draw_triangle(
                vec2(x(-12.0), fl - 30.0),
                vec2(x(-6.0), fl - 30.0),
                vec2(x(-9.0), fl - 25.0),
                rgb(230, 190, 40),
            );
        }
        Prop::ToolRack => {
            // Pegboard with hanging tools and a workbench.
            let (bx, w) = (lx(-22.0, 44.0), 44.0);
            draw_rectangle(bx, fl - 36.0, w, 18.0, rgb(150, 120, 80));
            let mut hy = fl - 34.0;
            while hy < fl - 19.0 {
                let mut hx = bx + 2.0;
                while hx < bx + w {
                    draw_circle(hx, hy, 0.6, rgb(90, 70, 46));
                    hx += 4.0;
                }
                hy += 4.0;
            }
            // Wrench, hammer, saw.
            draw_line(
                bx + 7.0,
                fl - 33.0,
                bx + 7.0,
                fl - 22.0,
                2.0,
                rgb(170, 170, 176),
            );
            draw_circle_lines(bx + 7.0, fl - 33.0, 2.5, 1.2, rgb(170, 170, 176));
            draw_line(
                bx + 17.0,
                fl - 33.0,
                bx + 17.0,
                fl - 21.0,
                1.6,
                rgb(120, 80, 50),
            );
            draw_rectangle(bx + 13.5, fl - 34.0, 7.0, 3.0, rgb(90, 90, 96));
            draw_triangle(
                vec2(bx + 26.0, fl - 32.0),
                vec2(bx + 38.0, fl - 32.0),
                vec2(bx + 26.0, fl - 24.0),
                rgb(190, 190, 196),
            );
            boxy(bx - 2.0, fl - 13.0, w + 4.0, 3.0, rgb(110, 84, 58));
            for lx_ in [bx, bx + w - 2.0] {
                draw_rectangle(lx_, fl - 10.0, 2.0, 10.0, rgb(70, 54, 40));
            }
            draw_rectangle(bx + 8.0, fl - 18.0, 10.0, 5.0, rgb(190, 40, 30)); // toolbox
        }
        Prop::Cart => {
            // Ore cart on a stub of track.
            draw_line(
                p.x - 26.0,
                fl - 1.0,
                p.x + 26.0,
                fl - 1.0,
                2.0,
                rgb(90, 80, 70),
            );
            let (bx, w) = (lx(-16.0, 30.0), 30.0);
            draw_triangle(
                vec2(bx, fl - 20.0),
                vec2(bx + w, fl - 20.0),
                vec2(bx + w - 4.0, fl - 6.0),
                rgb(110, 70, 40),
            );
            draw_triangle(
                vec2(bx, fl - 20.0),
                vec2(bx + 4.0, fl - 6.0),
                vec2(bx + w - 4.0, fl - 6.0),
                rgb(110, 70, 40),
            );
            draw_line(bx, fl - 20.0, bx + w, fl - 20.0, 2.0, rgb(150, 100, 60));
            for i in 0..5 {
                draw_circle(
                    bx + 5.0 + i as f32 * 5.0,
                    fl - 21.0 - (i % 2) as f32 * 2.0,
                    3.0,
                    rgb(70 + i as u8 * 6, 66, 62),
                );
            }
            for wx in [bx + 7.0, bx + w - 7.0] {
                draw_circle(wx, fl - 4.0, 3.5, rgb(40, 40, 42));
                draw_circle(wx, fl - 4.0, 1.2, rgb(120, 120, 120));
            }
        }
    }
}

// ---------------------------------------------------------------- spiral stair

/// Diamond-plate texture: a grid of raised diagonal studs.
fn diamond_plate(r: Rect, base: Color) {
    draw_rectangle(r.x, r.y, r.w, r.h, base);
    let hi = shade(base, 1.35);
    let lo = shade(base, 0.7);
    let mut row = 0;
    let mut y = r.y + 2.0;
    while y < r.y + r.h - 1.0 {
        let mut x = r.x + 2.0 + if row % 2 == 0 { 0.0 } else { 3.0 };
        while x < r.x + r.w - 2.0 {
            draw_line(x, y + 1.0, x + 2.0, y - 0.5, 1.0, hi);
            draw_line(x + 0.5, y + 1.5, x + 2.5, y, 0.8, lo);
            x += 6.0;
        }
        y += 3.0;
        row += 1;
    }
    draw_line(r.x, r.y, r.x + r.w, r.y, 1.0, hi);
    draw_line(r.x, r.y + r.h, r.x + r.w, r.y + r.h, 1.0, shade(base, 0.5));
}

/// One flight of the spiral stair: steel stringer, individual treads, balusters, two rails,
/// and diamond-plate landings bracketed to the wall at both ends.
fn stair(a: sc::Vec2, b: sc::Vec2, front: bool) {
    let (pa, pb) = (px(a), px(b));
    let k = if front { 1.0 } else { 0.55 };
    let steel = shade(rgb(128, 84, 50), k); // painted, worn steel
    let tread_c = shade(rgb(150, 150, 146), k);
    let rail_c = shade(rgb(170, 170, 174), k);
    if (pa.x - pb.x).abs() < 0.5 {
        // Hatch ladder up to the airlock: two rails and rungs.
        for dx in [-9.0, 9.0] {
            draw_line(
                pa.x + dx + 1.0,
                pa.y,
                pb.x + dx + 1.0,
                pb.y,
                3.0,
                rgba(0, 0, 0, 120),
            );
            draw_line(pa.x + dx, pa.y, pb.x + dx, pb.y, 3.0, rail_c);
        }
        let mut y = pb.y + 4.0;
        while y < pa.y {
            draw_line(pa.x - 9.0, y, pa.x + 9.0, y, 2.0, shade(rail_c, 0.75));
            draw_line(
                pa.x - 9.0,
                y - 1.0,
                pa.x + 9.0,
                y - 1.0,
                0.8,
                shade(rail_c, 1.2),
            );
            y += 8.0;
        }
        return;
    }
    let (lo, hi) = if pa.x < pb.x { (pa, pb) } else { (pb, pa) };
    let slope = (hi.y - lo.y) / (hi.x - lo.x);
    let y_at = |x: f32| lo.y + (x - lo.x) * slope;
    let (x0, x1) = (lo.x + 14.0, hi.x - 14.0);

    // Shadow cast on the back wall.
    draw_triangle(
        vec2(x0, y_at(x0) + 10.0),
        vec2(x1, y_at(x1) + 10.0),
        vec2(x1, y_at(x1) + 26.0),
        rgba(0, 0, 0, 60),
    );
    draw_triangle(
        vec2(x0, y_at(x0) + 10.0),
        vec2(x0, y_at(x0) + 26.0),
        vec2(x1, y_at(x1) + 26.0),
        rgba(0, 0, 0, 60),
    );
    // Stringer: a channel beam under the treads.
    let t = if front { 8.0 } else { 6.0 };
    let (s0, s1) = (vec2(x0, y_at(x0) + 2.0), vec2(x1, y_at(x1) + 2.0));
    draw_triangle(s0, s1, s1 + vec2(0.0, t), steel);
    draw_triangle(s0, s0 + vec2(0.0, t), s1 + vec2(0.0, t), steel);
    draw_line(s0.x, s0.y, s1.x, s1.y, 1.2, shade(steel, 1.4));
    draw_line(s0.x, s0.y + t, s1.x, s1.y + t, 1.2, shade(steel, 0.55));
    // Rivets along the stringer.
    let mut x = x0 + 6.0;
    while x < x1 {
        draw_circle(x, y_at(x) + 2.0 + t / 2.0, 0.9, shade(steel, 0.6));
        x += 14.0;
    }
    // Treads: horizontal steps with a lit nosing, risers in shadow.
    let step = 8.0;
    let n = ((x1 - x0) / step).floor() as i32;
    let rising_right = hi.y < lo.y;
    for i in 0..n {
        let sx = x0 + i as f32 * step;
        let ex = sx + step;
        let ty = if rising_right { y_at(ex) } else { y_at(sx) };
        draw_rectangle(sx, ty - 2.5, step, 3.0, tread_c);
        draw_line(sx, ty - 2.5, ex, ty - 2.5, 1.0, shade(tread_c, 1.3));
        let rx = if rising_right { ex } else { sx };
        let bottom = y_at(rx) + 2.0;
        draw_line(rx, ty, rx, bottom.max(ty), 1.0, shade(tread_c, 0.55));
    }
    // Railing: balusters, mid rail, handrail, heavier posts.
    let (top, mid) = (-22.0, -11.0);
    let mut x = x0;
    let mut i = 0;
    while x <= x1 {
        let y = y_at(x);
        let post = i % 5 == 0;
        draw_line(
            x,
            y - 2.0,
            x,
            y + top,
            if post { 2.0 } else { 0.9 },
            shade(rail_c, if post { 0.9 } else { 0.7 }),
        );
        x += 9.0;
        i += 1;
    }
    for (off, w) in [(top, 2.4), (mid, 1.2)] {
        draw_line(x0, y_at(x0) + off, x1, y_at(x1) + off, w, rail_c);
    }
    draw_line(
        x0,
        y_at(x0) + top - 1.0,
        x1,
        y_at(x1) + top - 1.0,
        0.8,
        shade(rail_c, 1.3),
    );

    // Landings: diamond plate on a bracket.
    for p in [pa, pb] {
        let w = 34.0;
        let r = Rect::new(p.x - w / 2.0, p.y - 2.0, w, 6.0);
        diamond_plate(r, shade(rgb(120, 122, 124), k));
        let wall_side = if p.x < PILLAR_X * TILE { -1.0 } else { 1.0 };
        let wx = p.x + wall_side * w / 2.0;
        draw_triangle(
            vec2(wx, r.y + r.h),
            vec2(wx - wall_side * 18.0, r.y + r.h),
            vec2(wx, r.y + r.h + 16.0),
            shade(steel, 0.8),
        );
        draw_line(
            wx - wall_side * 18.0,
            r.y + r.h,
            wx,
            r.y + r.h + 16.0,
            1.0,
            shade(steel, 1.3),
        );
    }
}

/// The central column: concrete drum segments with steel bands, bolts and a conduit.
fn pillar(light: &Lighting, vis: Vis) {
    let cx = PILLAR_X * TILE;
    let (y0, y1) = (HUD_H + 1.4 * TILE, HUD_H + WORLD_H);
    let (vy0, vy1) = (vis.y0.max(y0), vis.y1.min(y1));
    let w = 30.0;
    tiled(
        &light.concrete,
        Rect::new(cx - w / 2.0, vy0, w, vy1 - vy0),
        rgb(150, 150, 154),
        1.3,
    );
    for i in 0..10 {
        let t = (i as f32 + 0.5) / 10.0;
        let k = 0.35 + 0.65 * (PI * t).sin();
        draw_rectangle(
            cx - w / 2.0 + t * w - w / 20.0,
            vy0,
            w / 10.0 + 0.5,
            vy1 - vy0,
            Color::new(0.0, 0.0, 0.0, 1.0 - k),
        );
    }
    // Conduit clamped to the column.
    draw_rectangle(cx + 8.0, vy0, 4.0, vy1 - vy0, rgb(70, 72, 70));
    draw_rectangle(cx + 9.0, vy0, 1.0, vy1 - vy0, rgb(120, 124, 120));
    let mut y = ((vy0 - HUD_H) / TILE).floor() * TILE + HUD_H;
    while y < vy1 {
        if y > y0 {
            // Steel band with bolts.
            draw_rectangle(cx - w / 2.0 - 1.0, y - 3.0, w + 2.0, 6.0, rgb(64, 66, 70));
            draw_line(
                cx - w / 2.0 - 1.0,
                y - 3.0,
                cx + w / 2.0 + 1.0,
                y - 3.0,
                1.0,
                rgb(130, 132, 136),
            );
            for bx in [-10.0, -3.0, 4.0, 11.0] {
                draw_circle(cx + bx, y, 1.2, rgb(150, 150, 150));
            }
            // Conduit clamp.
            draw_rectangle(cx + 7.0, y + 14.0, 6.0, 3.0, rgb(90, 92, 94));
            // Drum seam halfway.
            draw_line(
                cx - w / 2.0,
                y + TILE / 2.0,
                cx + w / 2.0,
                y + TILE / 2.0,
                1.0,
                rgba(0, 0, 0, 70),
            );
        }
        y += TILE;
    }
}

// ---------------------------------------------------------------- airlock

/// The exit: a concrete bunker on the surface with a heavy round vault door, locking bars,
/// beacons, a seal-status panel and vent stacks; below it, the hatch tube the ladder climbs.
/// When the silo falls, the bars retract and the door swings open on its hinge.
pub fn airlock(game: &Game, time: f32, flash: f32) {
    let cx = (AIRLOCK[0].0 as f32 + 1.0) * TILE;
    let ground = HUD_H + TILE * 0.8; // horizon / top of the roof slab
    let lost = game.phase() == Phase::Lost;
    let concrete = rgb(124, 122, 116);

    // Hatch tube below ground, around the top of the ladder.
    let (t0, t1) = (HUD_H + TILE, HUD_H + 1.45 * TILE);
    draw_rectangle(cx - 18.0, t0 - 4.0, 36.0, t1 - t0 + 4.0, rgb(40, 42, 46));
    for x in [cx - 18.0, cx + 14.0] {
        draw_rectangle(x, t0 - 4.0, 4.0, t1 - t0 + 4.0, rgb(96, 98, 102));
        draw_rectangle(x + 1.0, t0 - 4.0, 1.0, t1 - t0 + 4.0, rgb(150, 152, 156));
    }
    let mut y = t0 + 2.0;
    while y < t1 {
        draw_rectangle(cx - 20.0, y, 40.0, 3.0, rgb(80, 82, 86));
        y += 9.0;
    }

    // Vent stacks.
    for vx in [cx - 64.0, cx + 58.0] {
        draw_rectangle(vx, ground - 46.0, 6.0, 20.0, rgb(84, 86, 88));
        draw_rectangle(vx - 2.0, ground - 48.0, 10.0, 3.0, rgb(110, 112, 114));
        draw_rectangle(vx + 1.0, ground - 46.0, 1.0, 20.0, rgb(140, 142, 144));
    }
    // Bunker: a sloped concrete berm.
    let (bw0, bw1, bt) = (200.0, 146.0, ground - 40.0);
    draw_triangle(
        vec2(cx - bw0 / 2.0, ground),
        vec2(cx - bw1 / 2.0, bt),
        vec2(cx + bw1 / 2.0, bt),
        concrete,
    );
    draw_triangle(
        vec2(cx - bw0 / 2.0, ground),
        vec2(cx + bw1 / 2.0, bt),
        vec2(cx + bw0 / 2.0, ground),
        concrete,
    );
    // Slope shading and a lit top edge.
    draw_triangle(
        vec2(cx - bw0 / 2.0, ground),
        vec2(cx - bw1 / 2.0, bt),
        vec2(cx - bw1 / 2.0, ground),
        shade(concrete, 0.8),
    );
    draw_triangle(
        vec2(cx + bw0 / 2.0, ground),
        vec2(cx + bw1 / 2.0, bt),
        vec2(cx + bw1 / 2.0, ground),
        shade(concrete, 0.7),
    );
    draw_line(
        cx - bw1 / 2.0,
        bt,
        cx + bw1 / 2.0,
        bt,
        2.0,
        shade(concrete, 1.25),
    );
    // Weathering streaks.
    for i in 0..9 {
        let x = cx - bw1 / 2.0 + 8.0 + i as f32 * 16.0;
        draw_line(
            x,
            bt + 2.0,
            x + 1.0,
            bt + 10.0 + (i % 3) as f32 * 6.0,
            1.0,
            rgba(70, 64, 56, 90),
        );
    }
    // Stencil.
    text_centered("AIRLOCK", cx, bt + 9.0, 9.0, rgb(60, 58, 52));

    // Door recess with a hazard-chevron frame.
    let (fw, fh) = (58.0, 36.0);
    let fr = Rect::new(cx - fw / 2.0, ground - fh, fw, fh);
    draw_rectangle(
        fr.x - 4.0,
        fr.y - 4.0,
        fr.w + 8.0,
        fr.h + 4.0,
        rgb(30, 30, 30),
    );
    let mut k = 0;
    let mut x = fr.x - 4.0;
    while x < fr.x + fr.w + 4.0 {
        let c = if k % 2 == 0 {
            rgb(220, 180, 40)
        } else {
            rgb(30, 30, 30)
        };
        draw_triangle(
            vec2(x, fr.y - 4.0),
            vec2(x + 5.0, fr.y - 4.0),
            vec2(x + 2.5, fr.y),
            c,
        );
        x += 5.0;
        k += 1;
    }
    let breach_red = flash > 0.0;
    draw_rectangle(
        fr.x,
        fr.y,
        fr.w,
        fr.h,
        if breach_red {
            rgb(90, 20, 18)
        } else {
            rgb(22, 22, 24)
        },
    );

    // The vault door.
    let dc = vec2(cx, fr.y + fh / 2.0 + 1.0);
    let r = 16.0;
    if lost {
        // Daylight through the open hatch; the door hangs open on its hinge.
        draw_circle(dc.x, dc.y, r, rgb(236, 232, 214));
        for i in 0..5 {
            let a = -PI / 2.0 + (i as f32 - 2.0) * 0.32;
            let tip = dc + vec2(a.cos(), a.sin()) * 70.0;
            let side = vec2(-a.sin(), a.cos()) * 9.0;
            draw_triangle(dc, tip + side, tip - side, rgba(255, 250, 220, 26));
        }
        let hinge = vec2(dc.x + r + 6.0, dc.y);
        draw_ellipse(hinge.x + 7.0, dc.y, 6.0, r, 0.0, rgb(96, 98, 102));
        draw_ellipse_lines(hinge.x + 7.0, dc.y, 6.0, r, 0.0, 1.5, rgb(60, 62, 66));
        draw_rectangle(hinge.x - 2.0, dc.y - 8.0, 5.0, 16.0, rgb(70, 72, 76));
    } else {
        draw_circle(dc.x, dc.y, r + 3.0, rgb(58, 60, 64));
        draw_circle(dc.x, dc.y, r, rgb(132, 136, 142));
        // Radial grooves and a bolt ring.
        for i in 0..12 {
            let a = i as f32 * PI / 6.0;
            let d = vec2(a.cos(), a.sin());
            draw_line(
                dc.x + d.x * 7.0,
                dc.y + d.y * 7.0,
                dc.x + d.x * (r - 2.0),
                dc.y + d.y * (r - 2.0),
                0.8,
                rgb(104, 108, 114),
            );
            draw_circle(
                dc.x + d.x * (r + 1.5),
                dc.y + d.y * (r + 1.5),
                1.1,
                rgb(170, 172, 176),
            );
        }
        draw_circle_lines(dc.x, dc.y, r - 1.0, 1.0, rgb(168, 172, 178));
        // Hub wheel with four spokes.
        let spin = (time * 0.2).sin() * 0.05;
        draw_circle_lines(dc.x, dc.y, 7.0, 2.0, rgb(80, 84, 90));
        for i in 0..4 {
            let a = spin + i as f32 * PI / 2.0 + PI / 4.0;
            draw_line(
                dc.x,
                dc.y,
                dc.x + a.cos() * 7.0,
                dc.y + a.sin() * 7.0,
                1.6,
                rgb(80, 84, 90),
            );
        }
        draw_circle(dc.x, dc.y, 2.5, rgb(60, 62, 66));
        // Locking bars across the door into the frame.
        for dy in [-7.0, 7.0] {
            draw_rectangle(
                fr.x + 2.0,
                dc.y + dy - 1.5,
                fr.w - 4.0,
                3.0,
                rgb(150, 150, 154),
            );
            draw_rectangle(
                fr.x + 2.0,
                dc.y + dy - 1.5,
                fr.w - 4.0,
                1.0,
                rgb(200, 200, 204),
            );
        }
        // Hinge block.
        draw_rectangle(dc.x + r + 3.0, dc.y - 9.0, 6.0, 18.0, rgb(70, 72, 76));
    }

    // Beacons on the top corners: alternate while sealed, solid red once breached.
    for (i, bx) in [cx - bw1 / 2.0 + 6.0, cx + bw1 / 2.0 - 6.0]
        .iter()
        .enumerate()
    {
        let on = lost || ((time * 2.0) as i32 + i as i32) % 2 == 0;
        let c = if lost {
            rgb(255, 50, 40)
        } else if on {
            rgb(255, 170, 40)
        } else {
            rgb(110, 70, 20)
        };
        draw_rectangle(bx - 4.0, bt - 2.0, 8.0, 3.0, rgb(50, 50, 52));
        draw_circle(*bx, bt - 4.0, 3.5, c);
        draw_circle(bx - 1.0, bt - 5.0, 1.0, rgba(255, 255, 255, 120));
    }

    // Seal-status panel beside the door: one light per remaining seal.
    let n = sc::stats::START_SEALS;
    let panel = Rect::new(fr.x - 40.0, fr.y + 4.0, 30.0, 26.0);
    draw_rectangle(panel.x, panel.y, panel.w, panel.h, rgb(28, 30, 34));
    draw_rectangle_lines(panel.x, panel.y, panel.w, panel.h, 1.0, rgb(90, 92, 96));
    for i in 0..n {
        let (col, row) = (i % 5, i / 5);
        let x = panel.x + 5.0 + col as f32 * 5.0;
        let y = panel.y + 5.0 + row as f32 * 5.5;
        let lit = i < game.seals();
        let c = if !lit {
            rgb(50, 22, 20)
        } else if game.seals() > n / 2 {
            rgb(90, 230, 110)
        } else if game.seals() > n / 4 {
            rgb(240, 200, 60)
        } else {
            rgb(255, 70, 50)
        };
        draw_rectangle(x - 1.5, y - 1.5, 3.0, 3.0, c);
    }
    // Intercom grille on the other side.
    let grille = Rect::new(fr.x + fr.w + 12.0, fr.y + 8.0, 18.0, 18.0);
    draw_rectangle(grille.x, grille.y, grille.w, grille.h, rgb(70, 72, 74));
    for i in 0..5 {
        draw_line(
            grille.x + 2.0,
            grille.y + 3.0 + i as f32 * 3.0,
            grille.x + grille.w - 2.0,
            grille.y + 3.0 + i as f32 * 3.0,
            1.0,
            rgb(30, 30, 32),
        );
    }
}

// ---------------------------------------------------------------- towers

/// Shaded quad helper: four corners, base colour, with a darker lower edge.
fn quad(a: Vec2, b: Vec2, c: Vec2, d: Vec2, col: Color) {
    draw_triangle(a, b, c, col);
    draw_triangle(a, c, d, col);
}

/// Sandbag: a squashed rounded lump with a seam and a highlight.
fn sandbag(c: Vec2, w: f32, h: f32, alpha: f32) {
    let base = with_alpha(rgb(152, 134, 98), alpha);
    draw_ellipse(
        c.x,
        c.y + 1.0,
        w / 2.0,
        h / 2.0,
        0.0,
        with_alpha(rgb(96, 82, 58), alpha),
    );
    draw_ellipse(c.x, c.y, w / 2.0, h / 2.0, 0.0, base);
    draw_ellipse(
        c.x - w * 0.12,
        c.y - h * 0.18,
        w * 0.28,
        h * 0.18,
        0.0,
        with_alpha(rgb(190, 172, 132), alpha),
    );
    draw_line(
        c.x,
        c.y - h * 0.4,
        c.x,
        c.y + h * 0.4,
        0.8,
        with_alpha(rgb(110, 94, 66), alpha),
    );
}

/// The four kinds of IT defence, each with its own silhouette so they read at a glance:
/// Where a gas launcher's tube ends, and the direction it points (lobbed fire: always up and
/// toward the target). Matches the tube `tower_icon` draws.
pub fn gas_muzzle(c: Vec2, aim: Option<Vec2>) -> (Vec2, Vec2) {
    let dx = aim.map_or(1.0, |p| p.x - c.x);
    let facing = if dx < 0.0 { -1.0 } else { 1.0 };
    let ad = vec2(facing * 0.55, -0.83).normalize();
    let base = vec2(c.x + 2.0 * facing, c.y + 20.0 - 12.0);
    (base + ad * 19.0, ad)
}

/// Deputy = a guard with a riot shield, Jammer = a tall radio mast, Gas = a squat mortar pit,
/// Marksman = a long sniper nest. `c` is the tile centre; `aim` the target; `recoil` 0..1.
pub fn tower_icon(kind: TowerKind, c: Vec2, level: u8, aim: Option<Vec2>, recoil: f32, alpha: f32) {
    let a = |col: Color| with_alpha(col, alpha);
    let col = tower_color(kind);
    let outline = a(rgb(14, 16, 20));
    let angle = aim.map_or(-0.35, |p| (p.y - c.y).atan2(p.x - c.x));
    let d = vec2(angle.cos(), angle.sin());
    let facing = if d.x < 0.0 { -1.0 } else { 1.0 };
    let back = recoil * 3.5;
    let line = |p: Vec2, q: Vec2, w: f32, col: Color| draw_line(p.x, p.y, q.x, q.y, w, a(col));
    let ground = c.y + 20.0;
    draw_ellipse(c.x, ground, 22.0, 4.0, 0.0, a(rgba(0, 0, 0, 120)));

    match kind {
        TowerKind::Deputy => {
            // Steel deck plate.
            draw_rectangle(c.x - 16.0, ground - 4.0, 32.0, 4.0, a(rgb(84, 90, 98)));
            draw_rectangle(c.x - 16.0, ground - 4.0, 32.0, 1.0, a(rgb(140, 146, 154)));
            // The deputy: boots, legs, blue uniform, belt, badge, cap.
            let f = facing;
            let hip = vec2(c.x - 2.0 * f, ground - 15.0);
            for (dx, k) in [(-2.5, 0.75), (2.5, 1.0)] {
                line(
                    hip + vec2(dx, 0.0),
                    vec2(c.x - 2.0 * f + dx * 1.3, ground - 4.0),
                    3.6,
                    shade(rgb(36, 44, 70), k),
                );
                draw_rectangle(
                    c.x - 2.0 * f + dx * 1.3 - 2.0 + f,
                    ground - 5.5,
                    4.5,
                    2.5,
                    a(rgb(20, 20, 22)),
                );
            }
            let sh = hip - vec2(0.0, 13.0);
            quad(
                hip - vec2(5.0, 0.0),
                hip + vec2(5.0, 0.0),
                sh + vec2(6.0, 0.0),
                sh - vec2(6.0, 0.0),
                a(shade(col, 0.75)),
            );
            quad(
                hip - vec2(5.0, 0.0),
                hip - vec2(1.0, 0.0),
                sh - vec2(1.5, 0.0),
                sh - vec2(6.0, 0.0),
                a(shade(col, 0.6)),
            );
            draw_rectangle(hip.x - 5.0, hip.y - 2.5, 10.0, 2.0, a(rgb(30, 26, 22)));
            draw_poly(
                sh.x + 2.5 * f,
                sh.y + 4.0,
                5,
                1.8,
                0.0,
                a(rgb(240, 210, 90)),
            );
            let head = sh - vec2(-f, 5.0);
            draw_circle(head.x, head.y, 4.3, a(rgb(226, 190, 154)));
            draw_rectangle(head.x - 4.8, head.y - 5.0, 9.6, 3.2, a(shade(col, 0.55)));
            draw_rectangle(
                head.x - 1.0 + 3.0 * f,
                head.y - 2.6,
                5.0,
                1.4,
                a(shade(col, 0.45)),
            );
            draw_circle(head.x + 2.2 * f, head.y - 0.3, 0.7, a(rgb(30, 24, 20)));
            if level >= 2 {
                // Helmet visor instead of a cap.
                draw_rectangle(head.x - 4.8, head.y - 5.5, 9.6, 4.0, a(rgb(50, 56, 70)));
                draw_rectangle(
                    head.x - 1.0 + 1.5 * f,
                    head.y - 1.8,
                    4.0 * f.abs(),
                    2.0,
                    a(rgba(120, 190, 255, 200)),
                );
            }
            // Rifle at the shoulder, aimed at the target.
            let grip = sh + vec2(2.0 * f, 3.0) - d * back;
            let tip = grip + d * 17.0;
            line(grip - d * 5.0, grip, 3.0, rgb(90, 62, 40));
            line(grip, tip, 2.2, rgb(40, 42, 46));
            line(tip - d * 3.0, tip, 3.2, rgb(26, 28, 30));
            draw_circle(grip.x, grip.y, 1.6, a(rgb(226, 190, 154)));
            // Riot shield on the near side.
            let sx = c.x + 7.0 * f;
            let shield = Rect::new(sx - 3.5, ground - 26.0, 7.0, 21.0);
            draw_rectangle(
                shield.x,
                shield.y,
                shield.w,
                shield.h,
                a(rgba(150, 190, 230, 150)),
            );
            draw_rectangle_lines(
                shield.x,
                shield.y,
                shield.w,
                shield.h,
                1.2,
                a(rgb(60, 70, 90)),
            );
            draw_rectangle(
                shield.x + 1.0,
                shield.y + 7.0,
                5.0,
                2.0,
                a(rgb(240, 240, 240)),
            );
            draw_line(
                shield.x + 1.5,
                shield.y + 2.0,
                shield.x + 1.5,
                shield.y + 12.0,
                0.8,
                a(rgba(255, 255, 255, 140)),
            );
            if level >= 3 {
                // Floodlight on a pole: the post is fully kitted out.
                line(
                    vec2(c.x - 12.0 * f, ground - 3.0),
                    vec2(c.x - 12.0 * f, ground - 34.0),
                    1.4,
                    rgb(70, 74, 80),
                );
                draw_rectangle(
                    c.x - 12.0 * f - 4.0,
                    ground - 38.0,
                    8.0,
                    5.0,
                    a(rgb(60, 64, 70)),
                );
                draw_rectangle(
                    c.x - 12.0 * f - 3.0,
                    ground - 34.0,
                    6.0,
                    1.5,
                    a(rgb(255, 240, 190)),
                );
            }
        }
        TowerKind::Jammer => {
            // Tripod legs, a crate-shaped transmitter, a tall lattice mast with a dish.
            for dx in [-14.0, 0.0, 14.0] {
                line(
                    vec2(c.x, ground - 12.0),
                    vec2(c.x + dx, ground),
                    2.0,
                    rgb(70, 74, 82),
                );
            }
            let body = Rect::new(c.x - 11.0, ground - 22.0, 22.0, 13.0);
            draw_rectangle(body.x, body.y, body.w, body.h, a(shade(col, 0.55)));
            draw_rectangle(body.x, body.y, body.w, 2.0, a(shade(col, 0.95)));
            draw_rectangle(
                body.x + body.w - 3.0,
                body.y,
                3.0,
                body.h,
                a(shade(col, 0.38)),
            );
            draw_rectangle_lines(body.x, body.y, body.w, body.h, 1.0, outline);
            for (i, dx) in [4.0, 10.0].iter().enumerate() {
                draw_circle(body.x + dx, body.y + 7.5, 2.2, a(rgb(220, 220, 210)));
                let t = get_time() as f32 * (1.3 + i as f32);
                line(
                    vec2(body.x + dx, body.y + 7.5),
                    vec2(
                        body.x + dx + t.sin() * 1.8,
                        body.y + 7.5 - t.cos().abs() * 1.8,
                    ),
                    0.7,
                    rgb(40, 40, 40),
                );
            }
            for i in 0..3 {
                draw_rectangle(
                    body.x + 14.0,
                    body.y + 4.0 + i as f32 * 3.0,
                    5.0,
                    1.5,
                    a(rgb(30, 30, 34)),
                );
            }
            // Lattice mast.
            let (m0, top) = (
                vec2(c.x + 4.0, body.y),
                vec2(c.x + 4.0, body.y - 30.0 - 4.0 * level as f32),
            );
            line(m0 + vec2(-3.0, 0.0), top, 1.4, rgb(170, 170, 180));
            line(m0 + vec2(3.0, 0.0), top, 1.4, rgb(170, 170, 180));
            let segs = 5;
            for i in 0..segs {
                let t0 = i as f32 / segs as f32;
                let t1 = (i + 1) as f32 / segs as f32;
                let w0 = 3.0 * (1.0 - t0);
                let w1 = 3.0 * (1.0 - t1);
                let y0 = m0.y + (top.y - m0.y) * t0;
                let y1 = m0.y + (top.y - m0.y) * t1;
                line(
                    vec2(m0.x - w0, y0),
                    vec2(m0.x + w1, y1),
                    0.8,
                    rgb(140, 140, 150),
                );
                line(
                    vec2(m0.x - w0, y0),
                    vec2(m0.x + w0, y0),
                    0.8,
                    rgb(140, 140, 150),
                );
            }
            // Dish on the mast and a whip antenna.
            let dish = vec2(m0.x - 7.0, m0.y - 16.0);
            draw_ellipse(dish.x, dish.y, 3.0, 6.5, 0.4, a(rgb(210, 210, 216)));
            draw_ellipse(dish.x + 0.8, dish.y, 1.8, 5.0, 0.4, a(rgb(150, 150, 160)));
            line(dish, vec2(m0.x, dish.y + 2.0), 1.0, rgb(120, 120, 130));
            if level >= 2 {
                let d2 = vec2(m0.x + 7.0, m0.y - 24.0);
                draw_ellipse(d2.x, d2.y, 2.5, 5.0, -0.4, a(rgb(210, 210, 216)));
                line(d2, vec2(m0.x, d2.y + 2.0), 1.0, rgb(120, 120, 130));
            }
            // Pulsing emitter rings at the tip.
            let t = (get_time() as f32 * 1.5).fract();
            draw_circle_lines(
                top.x,
                top.y,
                3.0 + t * 9.0,
                1.2,
                a(with_alpha(col, 1.0 - t)),
            );
            let blink = (get_time() as f32 * 4.0).sin() > 0.0;
            draw_circle(
                top.x,
                top.y,
                2.2,
                a(if blink {
                    rgb(255, 120, 255)
                } else {
                    rgb(110, 40, 120)
                }),
            );
        }
        TowerKind::Gas => {
            // Low mortar pit: a ring of sandbags with a fat tube and a canister rack.
            for (i, dx) in [-14.0, -5.0, 5.0, 14.0].iter().enumerate() {
                sandbag(
                    vec2(c.x + dx, ground - 3.5 - (i % 2) as f32 * 0.5),
                    11.0,
                    7.0,
                    alpha,
                );
            }
            for dx in [-9.5, 0.0, 9.5] {
                sandbag(vec2(c.x + dx, ground - 9.0), 10.0, 6.5, alpha);
            }
            // Canister rack behind.
            for i in 0..3 {
                let x = c.x - 16.0 * facing + i as f32 * 4.5 * facing;
                draw_rectangle(x - 2.0, ground - 22.0, 4.0, 10.0, a(rgb(210, 190, 60)));
                draw_rectangle(x - 2.0, ground - 18.0, 4.0, 1.6, a(rgb(30, 30, 30)));
                draw_rectangle(x - 1.0, ground - 24.0, 2.0, 2.0, a(rgb(90, 90, 90)));
            }
            // The tube: lobbed fire, so it always points up-and-toward the target.
            let ad = vec2(d.x.signum() * 0.55, -0.83).normalize();
            let base = vec2(c.x + 2.0 * facing, ground - 12.0);
            let tip = base + ad * (19.0 - back);
            line(base, tip, 9.0, shade(col, 0.45));
            line(
                base + vec2(-ad.y, ad.x) * 2.0,
                tip + vec2(-ad.y, ad.x) * 2.0,
                2.5,
                shade(col, 0.85),
            );
            for t in [0.3, 0.7] {
                let p = base.lerp(tip, t);
                let n = vec2(-ad.y, ad.x);
                line(p - n * 5.0, p + n * 5.0, 2.2, rgb(220, 190, 40));
            }
            draw_ellipse(
                tip.x,
                tip.y,
                4.5,
                2.2,
                ad.y.atan2(ad.x) + PI / 2.0,
                a(rgb(20, 22, 18)),
            );
            if level >= 2 {
                // A second tube.
                let b2 = base + vec2(-7.0 * facing, 2.0);
                let t2 = b2 + ad * 14.0;
                line(b2, t2, 6.0, shade(col, 0.4));
            }
            // Hazard skull sign.
            draw_triangle(
                vec2(c.x - 3.0, ground - 2.5),
                vec2(c.x + 3.0, ground - 2.5),
                vec2(c.x, ground - 7.5),
                a(rgb(230, 200, 40)),
            );
        }
        TowerKind::Marksman => {
            // Raised nest: timber legs, a deck, a long sandbag wall, a prone sniper.
            for dx in [-16.0, 16.0] {
                line(
                    vec2(c.x + dx, ground),
                    vec2(c.x + dx * 0.8, ground - 10.0),
                    2.4,
                    rgb(110, 80, 50),
                );
            }
            line(
                vec2(c.x - 16.0, ground - 3.0),
                vec2(c.x + 14.0, ground - 9.0),
                1.2,
                rgb(90, 64, 40),
            );
            draw_rectangle(c.x - 20.0, ground - 13.0, 40.0, 3.5, a(rgb(120, 88, 56)));
            draw_rectangle(c.x - 20.0, ground - 13.0, 40.0, 1.0, a(rgb(160, 120, 80)));
            // Prone sniper (behind the bags, facing the target side).
            let f = facing;
            let body0 = vec2(c.x - 12.0 * f, ground - 16.0);
            let body1 = vec2(c.x + 4.0 * f, ground - 17.0);
            line(body0, body1, 5.0, rgb(70, 66, 50));
            draw_circle(body1.x + 2.0 * f, body1.y - 2.0, 3.5, a(rgb(220, 186, 150)));
            draw_rectangle(
                body1.x + 2.0 * f - 4.0,
                body1.y - 6.5,
                8.0,
                3.0,
                a(shade(col, 0.8)),
            ); // beret
            // Long rifle with a scope.
            let grip = body1 + vec2(3.0 * f, 0.0) - d * back;
            let tip = grip + d * 26.0;
            line(grip - d * 6.0, grip, 3.0, rgb(110, 76, 46));
            line(grip, tip, 1.8, rgb(36, 38, 42));
            let n = vec2(-d.y, d.x) * -f;
            line(
                grip + d * 2.0 + n * 2.5,
                grip + d * 10.0 + n * 2.5,
                2.8,
                rgb(24, 26, 30),
            );
            draw_circle(
                grip.x + d.x * 10.0 + n.x * 2.5,
                grip.y + d.y * 10.0 + n.y * 2.5,
                1.2,
                a(rgb(120, 190, 255)),
            );
            // Front sandbag wall.
            for dx in [-14.0, -4.0, 6.0, 15.0] {
                sandbag(vec2(c.x + dx * f, ground - 13.5), 11.0, 6.0, alpha);
            }
            if level >= 2 {
                // Camo netting over the nest.
                for i in 0..6 {
                    let x = c.x - 18.0 + i as f32 * 7.0;
                    draw_triangle(
                        vec2(x, ground - 26.0),
                        vec2(x + 7.0, ground - 26.0),
                        vec2(x + 3.5, ground - 21.0),
                        a(rgba(70, 90, 50, 210)),
                    );
                }
                line(
                    vec2(c.x - 18.0, ground - 26.0),
                    vec2(c.x + 24.0, ground - 26.0),
                    1.0,
                    rgb(60, 70, 40),
                );
            }
        }
    }
    // Rank chevrons on a small plate under the post.
    let plate_w = level as f32 * 10.0 + 4.0;
    draw_rectangle(
        c.x - plate_w / 2.0,
        ground + 2.0,
        plate_w,
        6.0,
        a(rgb(26, 28, 32)),
    );
    for i in 0..level {
        let x = c.x - (level as f32 - 1.0) * 5.0 + i as f32 * 10.0;
        let y = ground + 3.0;
        draw_triangle(
            vec2(x - 4.0, y),
            vec2(x + 4.0, y),
            vec2(x, y + 4.0),
            a(rgb(250, 210, 70)),
        );
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
        // Identification pad in the tower's colour.
        let tc = tower_color(t.kind);
        draw_rectangle(c.x - 20.0, c.y + 19.0, 40.0, 3.0, with_alpha(tc, 0.85));
        draw_rectangle(
            c.x - 20.0,
            c.y + 19.0,
            40.0,
            1.0,
            with_alpha(shade(tc, 1.4), 0.9),
        );
        tower_icon(t.kind, c, t.level, t.aim.map(px), recoil, 1.0);
        if t.hp < t.max_hp {
            hp_bar(vec2(c.x, c.y - 30.0), 34.0, t.hp / t.max_hp);
        }
        if recoil > 0.5 && matches!(t.kind, TowerKind::Deputy | TowerKind::Marksman) {
            // Muzzle flash where each gun actually ends.
            let a = t.aim.map(px).unwrap_or(c);
            let dir = (a - c).normalize_or_zero();
            let muzzle = match t.kind {
                TowerKind::Deputy => c + vec2(0.0, -8.0) + dir * 19.0,
                _ => c + vec2(0.0, 3.0) + dir * 27.0,
            };
            draw_circle(muzzle.x, muzzle.y, 3.5, rgba(255, 230, 140, 230));
            draw_circle(muzzle.x, muzzle.y, 1.8, rgba(255, 255, 230, 255));
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
/// Interior of a multi-tile room: from the top of its first row to the floor of its last.
fn room_rect(tiles: &[(i32, i32)]) -> Rect {
    let c0 = tiles.iter().map(|t| t.0).min().unwrap();
    let c1 = tiles.iter().map(|t| t.0).max().unwrap() + 1;
    let r0 = tiles.iter().map(|t| t.1).min().unwrap();
    let r1 = tiles.iter().map(|t| t.1).max().unwrap() + 1;
    Rect::new(
        c0 as f32 * TILE,
        HUD_H + r0 as f32 * TILE,
        (c1 - c0) as f32 * TILE,
        (r1 - r0) as f32 * TILE - 8.0,
    )
}

/// A thick façade frame and a header sign spanning the room, so key rooms read at a glance.
fn facade(r: Rect, accent: Color, title: &str, subtitle: &str, title_c: Color) {
    let h = 18.0;
    draw_rectangle(r.x, r.y, r.w, h, rgb(18, 20, 26));
    draw_rectangle(r.x, r.y + h - 2.0, r.w, 2.0, accent);
    text(title, r.x + 8.0, r.y + 14.5, 16.0, title_c);
    let tw = measure(title, 16.0);
    text(
        subtitle,
        r.x + 14.0 + tw,
        r.y + 13.0,
        9.0,
        shade(title_c, 0.75),
    );
    for (x, w) in [(r.x, 4.0), (r.x + r.w - 4.0, 4.0)] {
        draw_rectangle(x, r.y, w, r.h, shade(accent, 0.55));
        draw_rectangle(x + 1.0, r.y, 1.0, r.h, accent);
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h + 8.0, 2.0, accent);
}

/// IT's server hall: two floors joined by a mezzanine. Tiled walls, server racks, the camera
/// wall IT watches the silo through, cable trays, a glass partition and a keypad door.
/// Goes dark, cracked and smoking when it falls.
fn it_room(game: &Game, time: f32) {
    let r = room_rect(&sc::map::IT_ROOM);
    let down = game.cameras_down();
    let fl = r.y + r.h;
    let mid = r.y + TILE; // mezzanine level (the floor between the two rows)
    // Wall tiles.
    let wall = if down {
        rgb(66, 70, 78)
    } else {
        rgb(172, 186, 200)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, wall);
    let mut x = r.x;
    while x < r.x + r.w {
        draw_line(x, r.y, x, fl, 1.0, shade(wall, 0.86));
        x += 8.0;
    }
    let mut y = r.y;
    while y < fl {
        draw_line(r.x, y, r.x + r.w, y, 1.0, shade(wall, 0.86));
        y += 8.0;
    }
    // Raised access floor on both levels.
    for base in [mid - 8.0, fl] {
        for i in 0..(r.w / 8.0) as i32 {
            let c = if i % 2 == 0 {
                rgb(118, 128, 144)
            } else {
                rgb(98, 108, 124)
            };
            draw_rectangle(r.x + i as f32 * 8.0, base - 5.0, 8.0, 5.0, c);
        }
    }
    // Mezzanine grating and railing.
    draw_rectangle(r.x + 4.0, mid - 8.0, r.w - 8.0, 3.0, rgb(70, 76, 86));
    let mut x = r.x + 6.0;
    while x < r.x + r.w - 4.0 {
        draw_line(x, mid - 8.0, x, mid - 18.0, 0.8, rgb(90, 96, 106));
        x += 6.0;
    }
    draw_line(
        r.x + 4.0,
        mid - 18.0,
        r.x + r.w - 4.0,
        mid - 18.0,
        1.5,
        rgb(130, 136, 146),
    );

    // Upper level: the camera wall (2x3 feeds of the stairwell) and two racks.
    for j in 0..2 {
        for i in 0..3 {
            let m = Rect::new(
                r.x + 10.0 + i as f32 * 17.0,
                r.y + 22.0 + j as f32 * 13.0,
                15.0,
                11.0,
            );
            draw_rectangle(m.x - 1.0, m.y - 1.0, m.w + 2.0, m.h + 2.0, rgb(30, 32, 36));
            if down {
                // Static.
                for k in 0..8 {
                    let n = ((time * 30.0) as u32)
                        .wrapping_mul(2_654_435_761)
                        .wrapping_add(k * 977 + (i + j * 3) * 131);
                    let g = (40 + n % 120) as u8;
                    draw_rectangle(
                        m.x + (n % 13) as f32,
                        m.y + (n / 13 % 9) as f32,
                        2.0,
                        1.5,
                        rgb(g, g, g),
                    );
                }
            } else {
                draw_rectangle(m.x, m.y, m.w, m.h, rgb(40, 70, 56));
                // A tiny stair diagonal, with a dot climbing it.
                draw_line(
                    m.x + 1.0,
                    m.y + m.h - 2.0,
                    m.x + m.w - 1.0,
                    m.y + 2.0,
                    1.0,
                    rgb(120, 200, 140),
                );
                let t = (time * 0.3 + (i + j * 3) as f32 * 0.17).fract();
                draw_circle(
                    m.x + 1.0 + t * (m.w - 2.0),
                    m.y + m.h - 2.0 - t * (m.h - 4.0),
                    1.0,
                    rgb(255, 120, 90),
                );
                draw_line(
                    m.x,
                    m.y + ((time * 8.0 + i as f32 * 3.0) % m.h),
                    m.x + m.w,
                    m.y + ((time * 8.0 + i as f32 * 3.0) % m.h),
                    0.5,
                    rgba(255, 255, 255, 40),
                );
            }
        }
    }
    // Server racks: two upstairs, four downstairs.
    let rack = |rx: f32, top: f32, h: f32, seed: usize| {
        let w = 15.0;
        draw_rectangle(rx + 2.0, top + 2.0, w, h, rgba(0, 0, 0, 60));
        draw_rectangle(rx, top, w, h, rgb(30, 34, 40));
        draw_rectangle(rx, top, w, 2.0, rgb(70, 76, 86));
        draw_rectangle(rx + w - 2.0, top, 2.0, h, rgb(22, 24, 28));
        let mut uy = top + 4.0;
        let mut u = 0;
        while uy < top + h - 3.0 {
            draw_rectangle(rx + 2.0, uy, w - 5.0, 3.0, rgb(46, 50, 58));
            if !down {
                let on = (time * (3.0 + (u % 3) as f32) + (seed * 5 + u) as f32).sin() > -0.2;
                let led = if (seed + u).is_multiple_of(4) {
                    rgb(255, 170, 40)
                } else {
                    rgb(70, 240, 110)
                };
                draw_rectangle(
                    rx + 3.0,
                    uy + 1.0,
                    1.5,
                    1.2,
                    if on { led } else { rgb(30, 50, 34) },
                );
                draw_rectangle(
                    rx + 6.0,
                    uy + 1.0,
                    1.5,
                    1.2,
                    if on {
                        rgb(80, 160, 255)
                    } else {
                        rgb(30, 40, 60)
                    },
                );
            }
            uy += 4.5;
            u += 1;
        }
    };
    for i in 0..2 {
        rack(
            r.x + 64.0 + i as f32 * 19.0,
            r.y + 22.0,
            mid - 13.0 - (r.y + 22.0),
            i,
        );
    }
    for i in 0..4 {
        rack(
            r.x + 22.0 + i as f32 * 21.0,
            mid + 6.0,
            fl - 5.0 - (mid + 6.0),
            i + 2,
        );
    }
    // Cable trays with cables dropping into the racks.
    for (ty, x0) in [(r.y + 19.0, r.x + 60.0), (mid + 2.0, r.x + 20.0)] {
        draw_rectangle(x0, ty, r.x + r.w - 8.0 - x0, 2.5, rgb(90, 96, 104));
    }
    // Keypad door, downstairs left.
    let door = Rect::new(r.x + 6.0, mid + 4.0, 12.0, fl - mid - 9.0);
    draw_rectangle(door.x, door.y, door.w, door.h, rgb(84, 92, 104));
    draw_rectangle_lines(door.x, door.y, door.w, door.h, 1.0, rgb(50, 54, 62));
    draw_rectangle(door.x + 8.0, door.y + 16.0, 3.0, 4.0, rgb(30, 32, 36));
    let key = if down {
        rgb(255, 60, 40)
    } else {
        rgb(60, 230, 90)
    };
    draw_rectangle(door.x + 8.5, door.y + 16.5, 2.0, 1.2, key);
    // Glass partition in front of the lower racks.
    let glass = Rect::new(r.x + 20.0, mid + 3.0, r.w - 26.0, fl - mid - 8.0);
    draw_rectangle(glass.x, glass.y, glass.w, glass.h, rgba(140, 200, 255, 28));
    for k in 0..3 {
        let x0 = glass.x + 10.0 + k as f32 * 26.0;
        draw_line(
            x0,
            glass.y + glass.h,
            x0 + 14.0,
            glass.y,
            1.2,
            rgba(255, 255, 255, 40),
        );
    }
    draw_rectangle_lines(glass.x, glass.y, glass.w, glass.h, 1.5, rgb(150, 160, 172));
    if down {
        let c = vec2(glass.x + 40.0, glass.y + 20.0);
        for a in [0.3f32, 1.4, 2.6, 3.7, 5.0] {
            let d = vec2(a.cos(), a.sin());
            draw_line(
                c.x,
                c.y,
                c.x + d.x * 20.0,
                c.y + d.y * 14.0,
                0.8,
                rgba(230, 240, 255, 170),
            );
        }
        for i in 0..8 {
            let t = (time * 0.8 + i as f32 * 0.29).fract();
            let p = vec2(
                r.x + 20.0 + (i * 13 % 80) as f32,
                fl - 10.0 - t * (r.h - 20.0),
            );
            draw_circle(
                p.x,
                p.y,
                2.0 + t * 6.0,
                rgba(110, 108, 104, ((1.0 - t) * 140.0) as u8),
            );
        }
        if (time * 9.0).sin() > 0.7 {
            draw_circle(r.x + 60.0, mid + 20.0, 2.5, rgb(255, 230, 140));
        }
    }
    // Façade and header.
    let strobe = down && (time * 6.0).sin() > 0.0;
    let accent = if strobe {
        rgb(255, 60, 40)
    } else if down {
        rgb(140, 50, 40)
    } else {
        rgb(80, 150, 240)
    };
    let title_c = if down {
        rgb(255, 110, 90)
    } else {
        rgb(170, 220, 255)
    };
    facade(
        r,
        accent,
        "IT",
        if down {
            "SERVER HALL - OFFLINE"
        } else {
            "SERVER HALL"
        },
        title_c,
    );
    if down {
        let label = format!("CAMERAS DOWN {:.0}s", game.it_down().ceil());
        draw_rectangle(
            r.x + 8.0,
            mid - 30.0,
            r.w - 16.0,
            13.0,
            rgba(60, 10, 10, 230),
        );
        text_centered(
            &label,
            r.x + r.w / 2.0,
            mid - 20.0,
            10.0,
            rgb(255, 130, 110),
        );
    } else if game.it_hp() < sc::stats::IT_HP {
        hp_bar(
            vec2(r.x + r.w / 2.0, r.y + 21.0),
            r.w - 24.0,
            game.it_hp() / sc::stats::IT_HP,
        );
    }
}

/// Supply's depot: a two-storey corrugated-steel warehouse with pallet racking, a ceiling hoist,
/// a wire weapons cage and a roll-up shutter. Once Supply joins the uprising the shutter is up,
/// the cage stands open and half empty, and the walls carry the rebels' paint.
fn supply_room(game: &Game, time: f32) {
    let r = room_rect(&sc::map::SUPPLY_ROOM);
    let joined = game.supply_joined();
    let fl = r.y + r.h;
    // Corrugated wall.
    let mut x = r.x;
    let mut i = 0;
    while x < r.x + r.w {
        let c = if i % 2 == 0 {
            rgb(112, 118, 110)
        } else {
            rgb(88, 94, 88)
        };
        draw_rectangle(x, r.y, 3.0, r.h, c);
        x += 3.0;
        i += 1;
    }
    for (rx, ry, rr) in [
        (r.x + 22.0, r.y + 70.0, 10.0),
        (r.x + 86.0, r.y + 34.0, 7.0),
    ] {
        draw_circle(rx, ry, rr, rgba(130, 70, 30, 90));
    }
    // Hazard-striped floor.
    let mut hx = r.x;
    let mut k = 0;
    while hx < r.x + r.w {
        let c = if k % 2 == 0 {
            rgb(230, 190, 40)
        } else {
            rgb(30, 30, 30)
        };
        draw_triangle(
            vec2(hx, fl),
            vec2(hx + 6.0, fl - 4.0),
            vec2(hx + 6.0, fl),
            c,
        );
        draw_triangle(
            vec2(hx, fl),
            vec2(hx, fl - 4.0),
            vec2(hx + 6.0, fl - 4.0),
            c,
        );
        hx += 6.0;
        k += 1;
    }
    // Pallet racking: orange uprights, blue beams, three levels of crates.
    let (rk0, rk1) = (r.x + 8.0, r.x + 58.0);
    for ux in [rk0, rk0 + 24.0, rk1] {
        draw_rectangle(ux, r.y + 22.0, 3.0, fl - r.y - 26.0, rgb(220, 120, 40));
    }
    for (lvl, by) in [fl - 4.0, fl - 30.0, fl - 56.0].iter().enumerate() {
        draw_rectangle(rk0, by - 2.0, rk1 - rk0 + 3.0, 3.0, rgb(50, 90, 160));
        if lvl == 2 {
            continue;
        }
        for (j, bx) in [rk0 + 4.0, rk0 + 28.0].iter().enumerate() {
            let open = joined && (lvl + j) % 2 == 0;
            let (cw, ch) = (18.0, 20.0);
            draw_rectangle(*bx, by - 2.0 - ch, cw, ch, rgb(150, 116, 70));
            draw_rectangle_lines(*bx, by - 2.0 - ch, cw, ch, 1.0, rgb(90, 66, 38));
            if open {
                draw_rectangle(*bx + 1.0, by - 2.0 - ch, cw - 2.0, 4.0, rgb(40, 30, 20));
                for g in 0..3 {
                    let gx = bx + 4.0 + g as f32 * 5.0;
                    draw_line(gx, by - ch, gx + 2.0, by - ch - 8.0, 1.4, rgb(30, 30, 32));
                }
            } else {
                draw_line(*bx, by - 2.0 - ch, bx + cw, by - 2.0, 0.8, rgb(110, 84, 50));
            }
            text_centered(
                &format!("S{}", 2 + lvl * 2 + j),
                bx + cw / 2.0,
                by - 8.0,
                8.0,
                rgb(40, 30, 20),
            );
        }
    }
    // Ceiling hoist with a crate on the hook.
    draw_rectangle(r.x + 6.0, r.y + 21.0, r.w - 12.0, 3.0, rgb(70, 72, 74));
    let hx = r.x + 82.0 + (time * 0.4).sin() * 6.0;
    draw_rectangle(hx - 5.0, r.y + 22.0, 10.0, 5.0, rgb(200, 160, 40));
    let hook_y = r.y + 46.0;
    let mut cy = r.y + 27.0;
    while cy < hook_y {
        draw_circle_lines(hx, cy, 1.4, 0.8, rgb(150, 150, 150));
        cy += 3.0;
    }
    draw_rectangle(hx - 8.0, hook_y, 16.0, 12.0, rgb(140, 110, 66));
    draw_rectangle_lines(hx - 8.0, hook_y, 16.0, 12.0, 1.0, rgb(80, 60, 34));
    // Weapons cage (lower right) behind a roll-up shutter.
    let cage = Rect::new(r.x + 68.0, fl - 40.0, 38.0, 36.0);
    draw_rectangle(cage.x, cage.y, cage.w, cage.h, rgb(36, 38, 36));
    let rifles = if joined { 2 } else { 5 };
    for j in 0..rifles {
        let gx = cage.x + 6.0 + j as f32 * 7.0;
        draw_line(
            gx,
            cage.y + cage.h - 3.0,
            gx + 2.0,
            cage.y + 6.0,
            1.6,
            rgb(26, 26, 28),
        );
        draw_line(
            gx,
            cage.y + cage.h - 3.0,
            gx - 1.0,
            cage.y + cage.h - 9.0,
            2.2,
            rgb(110, 76, 46),
        );
    }
    let mut gx = cage.x;
    while gx <= cage.x + cage.w {
        draw_line(gx, cage.y, gx, cage.y + cage.h, 0.6, rgb(150, 154, 150));
        gx += 4.0;
    }
    let shut = if joined { 4.0 } else { cage.h * 0.65 };
    let mut sy = cage.y;
    while sy < cage.y + shut {
        draw_rectangle(cage.x, sy, cage.w, 2.5, rgb(140, 144, 140));
        draw_line(
            cage.x,
            sy + 2.5,
            cage.x + cage.w,
            sy + 2.5,
            0.8,
            rgb(80, 84, 80),
        );
        sy += 3.0;
    }
    draw_rectangle(
        cage.x - 1.0,
        cage.y - 3.0,
        cage.w + 2.0,
        4.0,
        rgb(70, 72, 70),
    );
    draw_rectangle_lines(cage.x, cage.y, cage.w, cage.h, 1.5, rgb(170, 174, 170));
    if !joined {
        draw_rectangle(
            cage.x + cage.w / 2.0 - 2.0,
            cage.y + shut - 2.0,
            4.0,
            4.0,
            rgb(200, 170, 60),
        );
    }
    if joined {
        text(
            "FREE THE SILO",
            r.x + 10.0,
            fl - 64.0,
            9.0,
            rgb(220, 40, 30),
        );
        let pole = vec2(r.x + r.w - 10.0, r.y + 24.0);
        draw_line(
            pole.x,
            pole.y,
            pole.x,
            pole.y + 28.0,
            1.5,
            rgb(150, 130, 100),
        );
        let wave = (time * 5.0).sin() * 2.0;
        draw_triangle(
            pole,
            vec2(pole.x - 18.0, pole.y + 6.0 + wave),
            vec2(pole.x, pole.y + 12.0),
            rgb(220, 40, 30),
        );
    }
    let accent = if joined {
        rgb(220, 60, 40)
    } else {
        rgb(230, 170, 40)
    };
    facade(
        r,
        accent,
        "SUPPLY",
        if joined { "REBEL DEPOT" } else { "DEPOT" },
        if joined {
            rgb(255, 120, 90)
        } else {
            rgb(250, 210, 80)
        },
    );
}

/// A barricade across the stair. Level 1: crates and planks nailed together. Level 2: a
/// sandbag wall backed by corrugated steel. Level 3: a welded riot gate with spikes and hazard
/// stripes. Cracks and dents show as it weakens (`hp` 0..1); a wreck is a heap of debris.
pub fn barrier_icon(c: Vec2, s: f32, level: u8, hp: f32, broken: bool, alpha: f32) {
    let a = |col: Color| with_alpha(col, alpha);
    let line = |p: Vec2, q: Vec2, w: f32, col: Color| draw_line(p.x, p.y, q.x, q.y, w * s, a(col));
    let base = c + vec2(0.0, 6.0 * s);
    let (w, h) = (42.0 * s, 30.0 * s);
    let (l, r, top) = (base.x - w / 2.0, base.x + w / 2.0, base.y - h);
    let v = |x: f32, y: f32| vec2(base.x + x * s, base.y + y * s);

    if broken {
        // The wreck: splintered planks / torn bags / bent bars in a low heap.
        draw_ellipse(base.x, base.y, w * 0.55, 3.5 * s, 0.0, a(rgba(0, 0, 0, 110)));
        match level {
            1 => {
                for (x0, y0, x1, y1) in [(-18.0, -2.0, 2.0, -7.0), (-6.0, -1.0, 16.0, -4.0), (4.0, -8.0, 19.0, -1.0)] {
                    line(v(x0, y0), v(x1, y1), 3.0, rgb(112, 84, 50));
                    line(v(x0, y0 - 1.0), v(x1, y1 - 1.0), 0.8, rgb(150, 118, 76));
                }
                quad(v(-12.0, 0.0), v(-2.0, 0.0), v(-4.0, -6.0), v(-13.0, -5.0), a(rgb(96, 72, 44)));
            }
            2 => {
                for (x, y) in [(-13.0, -2.5), (-2.0, -2.0), (10.0, -3.0), (-7.0, -7.0), (5.0, -7.5)] {
                    sandbag(v(x, y), 11.0 * s, 6.0 * s, alpha);
                }
                quad(v(8.0, -1.0), v(19.0, -3.0), v(17.0, -12.0), v(9.0, -9.0), a(rgb(110, 116, 120)));
            }
            _ => {
                quad(v(-19.0, 0.0), v(19.0, 0.0), v(15.0, -6.0), v(-16.0, -4.0), a(rgb(74, 78, 84)));
                for x in [-12.0, -4.0, 6.0, 13.0] {
                    line(v(x, -3.0), v(x + 5.0, -13.0 + (x * 0.7).sin() * 3.0), 2.2, rgb(120, 124, 130));
                }
                for i in 0..4 {
                    let x = -16.0 + i as f32 * 8.0;
                    quad(v(x, 0.0), v(x + 4.0, 0.0), v(x + 5.0, -3.0), v(x + 1.0, -3.0), a(rgb(230, 190, 40)));
                }
            }
        }
        // Grey dust over it.
        for i in 0..5 {
            let x = -16.0 + i as f32 * 8.0;
            draw_circle(base.x + x * s, base.y - 1.0 * s, 1.2 * s, a(rgb(120, 116, 108)));
        }
        return;
    }

    draw_ellipse(base.x, base.y + 1.0, w * 0.55, 3.5 * s, 0.0, a(rgba(0, 0, 0, 110)));
    let hp = hp.clamp(0.0, 1.0);
    match level {
        1 => {
            // Two crates side by side.
            for (x0, x1, k) in [(l, base.x - 1.0 * s, 0.85), (base.x + 1.0 * s, r, 1.0)] {
                let rc = Rect::new(x0, top + 6.0 * s, x1 - x0, h - 6.0 * s);
                draw_rectangle(rc.x, rc.y, rc.w, rc.h, a(shade(rgb(132, 98, 58), k)));
                draw_rectangle_lines(rc.x, rc.y, rc.w, rc.h, 1.5 * s, a(rgb(70, 50, 28)));
                line(vec2(rc.x, rc.y), vec2(rc.x + rc.w, rc.y + rc.h), 1.5, rgb(84, 62, 36));
                line(vec2(rc.x, rc.y + rc.h), vec2(rc.x + rc.w, rc.y), 1.5, rgb(84, 62, 36));
                draw_rectangle(rc.x, rc.y, rc.w, 1.5 * s, a(rgb(170, 132, 86)));
            }
            // Planks nailed across.
            for (y0, y1, k) in [(-20.0, -10.0, 1.0), (-8.0, -16.0, 0.9)] {
                line(v(-23.0, y0), v(23.0, y1), 4.0, shade(rgb(150, 112, 66), k));
                line(v(-23.0, y0 - 1.5), v(23.0, y1 - 1.5), 0.8, rgb(184, 148, 98));
                for x in [-18.0, 18.0] {
                    let t = (x + 23.0) / 46.0;
                    draw_circle(base.x + x * s, base.y + (y0 + (y1 - y0) * t) * s, 0.9 * s, a(rgb(60, 60, 64)));
                }
            }
            // A chair jammed on top.
            line(v(-10.0, -24.0), v(-10.0, -32.0), 1.6, rgb(90, 66, 40));
            line(v(-10.0, -28.0), v(-3.0, -28.0), 1.8, rgb(110, 82, 50));
            line(v(-3.0, -28.0), v(-2.0, -24.0), 1.6, rgb(90, 66, 40));
        }
        2 => {
            // Corrugated steel sheet behind.
            let sheet = Rect::new(l + 3.0 * s, top - 6.0 * s, w - 6.0 * s, h);
            draw_rectangle(sheet.x, sheet.y, sheet.w, sheet.h, a(rgb(104, 110, 116)));
            let mut x = sheet.x + 2.0 * s;
            while x < sheet.x + sheet.w {
                draw_line(x, sheet.y, x, sheet.y + sheet.h, 1.2 * s, a(rgb(140, 146, 152)));
                x += 4.0 * s;
            }
            for (rx, ry) in [(0.12, 0.15), (0.88, 0.15)] {
                draw_circle(sheet.x + sheet.w * rx, sheet.y + sheet.h * ry, 1.0 * s, a(rgb(60, 62, 66)));
            }
            // Three staggered rows of sandbags.
            for (row, n) in [(0, 4), (1, 4), (2, 3)] {
                let y = base.y - 4.0 * s - row as f32 * 6.5 * s;
                let off = if row % 2 == 1 { 5.0 } else { 0.0 };
                for i in 0..n {
                    let x = l + (6.0 + off + i as f32 * 10.0) * s;
                    sandbag(vec2(x, y), 11.0 * s, 7.0 * s, alpha);
                }
            }
        }
        _ => {
            // Welded riot gate: frame, bars, a steel plate, hazard stripes and spikes.
            let steel = rgb(84, 90, 98);
            draw_rectangle(l, top - 2.0 * s, w, h + 2.0 * s, a(rgb(40, 42, 46)));
            for x in [-17.0, -8.5, 0.0, 8.5, 17.0] {
                line(v(x, 0.0), v(x, -32.0), 2.6, steel);
                line(v(x - 0.8, 0.0), v(x - 0.8, -32.0), 0.7, rgb(150, 156, 164));
                draw_triangle(v(x - 2.0, -32.0), v(x + 2.0, -32.0), v(x, -37.0), a(rgb(170, 176, 184)));
            }
            line(v(-21.0, -31.0), v(21.0, -31.0), 3.0, steel);
            let plate = Rect::new(l + 4.0 * s, top + 6.0 * s, w - 8.0 * s, 13.0 * s);
            draw_rectangle(plate.x, plate.y, plate.w, plate.h, a(rgb(118, 124, 132)));
            draw_rectangle(plate.x, plate.y, plate.w, 1.2 * s, a(rgb(170, 176, 184)));
            text_centered("IT", base.x, plate.y + plate.h * 0.78, 10.0 * s, a(rgb(40, 44, 50)));
            for (rx, ry) in [(0.06, 0.2), (0.94, 0.2), (0.06, 0.8), (0.94, 0.8)] {
                draw_circle(plate.x + plate.w * rx, plate.y + plate.h * ry, 0.9 * s, a(rgb(60, 62, 66)));
            }
            // Hazard stripes along the foot.
            draw_rectangle(l, base.y - 5.0 * s, w, 5.0 * s, a(rgb(30, 30, 30)));
            let mut x = 0.0;
            while x < 42.0 {
                quad(v(-21.0 + x, 0.0), v(-18.0 + x, 0.0), v(-15.0 + x, -5.0), v(-18.0 + x, -5.0), a(rgb(230, 190, 40)));
                x += 6.0;
            }
            line(v(-21.0, 0.0), v(-21.0, -32.0), 3.4, rgb(60, 64, 70));
            line(v(21.0, 0.0), v(21.0, -32.0), 3.4, rgb(60, 64, 70));
        }
    }
    // Damage: cracks, then holes.
    let crack = rgb(28, 22, 16);
    if hp < 0.7 {
        line(v(-9.0, -24.0), v(-4.0, -16.0), 1.4, crack);
        line(v(-4.0, -16.0), v(1.0, -12.0), 1.2, crack);
    }
    if hp < 0.45 {
        line(v(12.0, -22.0), v(7.0, -13.0), 1.4, crack);
        line(v(7.0, -13.0), v(11.0, -7.0), 1.2, crack);
        draw_circle(base.x + 4.0 * s, base.y - 19.0 * s, 2.0 * s, a(rgba(10, 10, 12, 200)));
    }
    if hp < 0.2 {
        line(v(-15.0, -10.0), v(-6.0, -4.0), 1.6, crack);
        draw_circle(base.x - 11.0 * s, base.y - 14.0 * s, 2.6 * s, a(rgba(10, 10, 12, 220)));
    }
    // Level pips.
    for i in 0..level {
        let x = base.x + (i as f32 - (level as f32 - 1.0) / 2.0) * 5.0 * s;
        draw_circle(x, base.y + 4.0 * s, 1.4 * s, a(rgb(250, 210, 80)));
    }
    if hp < 1.0 {
        hp_bar(vec2(base.x, top - 14.0 * s), w, hp);
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

/// A rebel, side-on, walking the way the stair goes (right on the front passes, left on the
/// back ones). Proportions: boots, two legs, torso, swinging arms, head, per-kind kit.
pub fn rebel(e: &sc::Enemy, front: bool, time: f32) {
    let leader = e.kind == EnemyKind::Leader;
    let s = 1.5 * if leader { 1.25 } else { 1.0 } * if front { 1.0 } else { 0.85 };
    let k = if front { 1.0 } else { 0.6 };
    let dir = if front { 1.0 } else { -1.0 }; // facing
    let c = |col: Color| shade(col, k);
    let base = px(e.pos) + vec2(e.lane * TILE * 0.9, -4.0);
    draw_ellipse(
        base.x,
        base.y + 1.0,
        7.0 * s,
        2.2 * s,
        0.0,
        rgba(0, 0, 0, 110),
    );

    // Walk cycle (stands still and works when blocked at a barrier).
    let walking = !e.blocked;
    let phase = time * 10.0 * e.effective_speed().max(0.3) + e.id as f32 * 1.7;
    let swing = if walking { phase.sin() } else { 0.0 };
    let bob = if walking {
        phase.sin().abs() * 0.8 * s
    } else {
        0.0
    };
    let hip = base - vec2(0.0, 8.5 * s + bob);
    let (skin, hair) = {
        let tones = [
            rgb(232, 196, 160),
            rgb(196, 150, 110),
            rgb(150, 104, 72),
            rgb(240, 210, 180),
        ];
        let hairs = [
            rgb(40, 30, 24),
            rgb(110, 70, 40),
            rgb(200, 170, 110),
            rgb(60, 60, 60),
        ];
        (tones[(e.id % 4) as usize], hairs[(e.id / 4 % 4) as usize])
    };
    let (shirt, pants, boots) = match e.kind {
        EnemyKind::Porter => (rgb(196, 170, 120), rgb(90, 80, 66), rgb(70, 50, 36)),
        EnemyKind::Mechanic => (rgb(214, 110, 36), rgb(196, 100, 34), rgb(40, 36, 34)),
        EnemyKind::Miner => (rgb(96, 80, 64), rgb(70, 62, 54), rgb(36, 32, 30)),
        EnemyKind::Leader => (rgb(150, 30, 30), rgb(50, 44, 44), rgb(30, 26, 24)),
    };

    // Legs (far leg darker) and boots.
    for (i, sgn) in [(0, -1.0), (1, 1.0)] {
        let kk = if i == 0 { 0.75 } else { 1.0 };
        let foot = base + vec2(sgn * swing * 3.2 * s, -0.5 * s);
        let knee = hip.lerp(foot, 0.5) + vec2(dir * 1.2 * s, 0.0);
        draw_line(hip.x, hip.y, knee.x, knee.y, 2.6 * s, c(shade(pants, kk)));
        draw_line(knee.x, knee.y, foot.x, foot.y, 2.4 * s, c(shade(pants, kk)));
        draw_rectangle(
            foot.x - 1.6 * s + dir * 0.8 * s,
            foot.y - 1.6 * s,
            3.6 * s,
            2.0 * s,
            c(shade(boots, kk)),
        );
    }
    // Torso: slightly wider at the shoulders.
    let (sw, hw, th) = (4.8 * s, 4.0 * s, 10.5 * s);
    let sh = hip - vec2(0.0, th);
    let lean = vec2(dir * 0.8 * s, 0.0);
    draw_triangle(
        hip - vec2(hw, 0.0),
        hip + vec2(hw, 0.0),
        sh + lean + vec2(sw, 0.0),
        c(shirt),
    );
    draw_triangle(
        hip - vec2(hw, 0.0),
        sh + lean - vec2(sw, 0.0),
        sh + lean + vec2(sw, 0.0),
        c(shirt),
    );
    // Belt.
    draw_line(
        hip.x - hw,
        hip.y - 0.8 * s,
        hip.x + hw,
        hip.y - 0.8 * s,
        1.2 * s,
        c(rgb(50, 36, 26)),
    );

    // Kit worn on the back (drawn before the near arm).
    let back = sh + lean + vec2(-dir * sw, 2.0 * s);
    match e.kind {
        EnemyKind::Porter => {
            // Pack frame with a bundled load.
            let bx = back.x - dir * 4.0 * s;
            draw_rectangle(
                bx - 3.0 * s,
                back.y - 2.0 * s,
                6.0 * s,
                10.0 * s,
                c(rgb(120, 90, 56)),
            );
            draw_line(
                bx - 3.0 * s,
                back.y + 2.0 * s,
                bx + 3.0 * s,
                back.y + 2.0 * s,
                0.8 * s,
                c(rgb(70, 50, 30)),
            );
            draw_circle(bx, back.y - 3.0 * s, 3.0 * s, c(rgb(170, 150, 110)));
        }
        EnemyKind::Miner => {
            // Pickaxe slung across the back.
            let a = back + vec2(-dir * 2.0 * s, 8.0 * s);
            let b = back + vec2(dir * 2.0 * s, -7.0 * s);
            draw_line(a.x, a.y, b.x, b.y, 1.2 * s, c(rgb(120, 86, 54)));
            draw_line(
                b.x - 4.0 * s,
                b.y + 1.0 * s,
                b.x + 4.0 * s,
                b.y - 1.0 * s,
                1.4 * s,
                c(rgb(150, 150, 156)),
            );
        }
        EnemyKind::Leader => {
            // Banner pole held high behind.
            let pole = back + vec2(0.0, 6.0 * s);
            let top = pole - vec2(0.0, 24.0 * s);
            draw_line(pole.x, pole.y, top.x, top.y, 1.2 * s, c(rgb(150, 130, 100)));
            let wave = (time * 6.0).sin() * 2.0;
            draw_triangle(
                top,
                top + vec2(-dir * 13.0 * s, 4.0 * s + wave),
                top + vec2(0.0, 9.0 * s),
                c(rgb(220, 40, 30)),
            );
            draw_circle(
                top.x - dir * 4.0 * s,
                top.y + 4.5 * s,
                1.4 * s,
                c(rgb(240, 220, 200)),
            );
        }
        EnemyKind::Mechanic => {}
    }

    // Head: neck, face, hair, per-kind headgear.
    let head = sh + lean - vec2(-dir * 0.6 * s, 4.2 * s);
    draw_rectangle(
        head.x - 1.0 * s,
        head.y + 2.5 * s,
        2.0 * s,
        2.0 * s,
        c(skin),
    );
    draw_circle(head.x, head.y, 3.4 * s, c(skin));
    draw_circle(
        head.x + dir * 1.8 * s,
        head.y - 0.6 * s,
        0.55 * s,
        c(rgb(30, 24, 20)),
    ); // eye
    match e.kind {
        EnemyKind::Miner => {
            draw_rectangle(
                head.x - 4.0 * s,
                head.y - 3.6 * s,
                8.0 * s,
                2.6 * s,
                c(rgb(220, 180, 40)),
            );
            draw_circle(head.x, head.y - 3.4 * s, 3.4 * s, c(rgb(220, 180, 40)));
            draw_circle(
                head.x + dir * 3.0 * s,
                head.y - 3.0 * s,
                1.2 * s,
                rgb(255, 250, 210),
            );
        }
        EnemyKind::Mechanic => {
            // Cap with a peak.
            draw_rectangle(
                head.x - 3.4 * s,
                head.y - 3.6 * s,
                6.8 * s,
                2.4 * s,
                c(rgb(60, 70, 90)),
            );
            draw_rectangle(
                head.x + dir * 2.0 * s - 1.5 * s,
                head.y - 1.6 * s,
                3.0 * s,
                0.9 * s,
                c(rgb(50, 58, 76)),
            );
        }
        EnemyKind::Leader => {
            draw_circle(head.x - dir * 0.8 * s, head.y - 1.6 * s, 3.0 * s, c(hair));
            draw_rectangle(
                head.x - 3.5 * s,
                head.y - 1.4 * s,
                7.0 * s,
                1.4 * s,
                c(rgb(230, 40, 30)),
            ); // headband
        }
        EnemyKind::Porter => {
            draw_circle(head.x - dir * 0.8 * s, head.y - 1.8 * s, 2.8 * s, c(hair));
        }
    }

    // Near arm, holding whatever Supply handed over.
    let shoulder = sh + lean + vec2(dir * 0.5 * s, 1.2 * s);
    if e.armed {
        // Rifle held across the chest, pointing ahead.
        let hand = shoulder + vec2(dir * 5.0 * s, 4.0 * s);
        draw_line(shoulder.x, shoulder.y, hand.x, hand.y, 2.0 * s, c(shirt));
        let stock = hand - vec2(dir * 5.0 * s, -s);
        let muzzle = hand + vec2(dir * 9.0 * s, -2.5 * s);
        draw_line(
            stock.x,
            stock.y,
            hand.x,
            hand.y,
            2.2 * s,
            c(rgb(110, 76, 46)),
        );
        draw_line(
            hand.x,
            hand.y,
            muzzle.x,
            muzzle.y,
            1.3 * s,
            c(rgb(40, 40, 44)),
        );
        draw_circle(hand.x, hand.y, 1.1 * s, c(skin));
    } else if e.tools {
        // Sledgehammer: swung at barricades, carried on the shoulder otherwise.
        let ang = if e.blocked {
            (time * 10.0 + e.id as f32).sin() * 1.2
        } else {
            -0.7
        };
        let hd = vec2(dir * ang.sin(), -ang.cos());
        let hand = shoulder + vec2(dir * 3.0 * s, 3.0 * s);
        draw_line(shoulder.x, shoulder.y, hand.x, hand.y, 2.0 * s, c(shirt));
        let head_p = hand + hd * 11.0 * s;
        draw_line(
            hand.x,
            hand.y,
            head_p.x,
            head_p.y,
            1.3 * s,
            c(rgb(130, 100, 70)),
        );
        draw_rectangle(
            head_p.x - 2.8 * s,
            head_p.y - 1.8 * s,
            5.6 * s,
            3.6 * s,
            c(rgb(140, 140, 146)),
        );
        draw_circle(hand.x, hand.y, 1.1 * s, c(skin));
    } else {
        let hand = shoulder + vec2(dir * (-swing) * 3.0 * s, 8.0 * s);
        draw_line(shoulder.x, shoulder.y, hand.x, hand.y, 2.0 * s, c(shirt));
        draw_circle(hand.x, hand.y, 1.1 * s, c(skin));
        if e.kind == EnemyKind::Mechanic {
            // Big wrench.
            draw_line(
                hand.x,
                hand.y,
                hand.x + dir * 2.0 * s,
                hand.y + 6.0 * s,
                1.4 * s,
                c(rgb(170, 170, 176)),
            );
            draw_circle_lines(
                hand.x + dir * 2.0 * s,
                hand.y + 6.5 * s,
                1.5 * s,
                0.8 * s,
                c(rgb(170, 170, 176)),
            );
        }
    }

    // Status: slowed (static crackle), health bar.
    if e.is_slowed() {
        for i in 0..3 {
            let a = time * 5.0 + i as f32 * 2.09;
            let p = head + vec2(a.cos() * 7.0 * s, a.sin() * 2.5 * s - 5.0 * s);
            draw_circle(p.x, p.y, 1.4, rgb(220, 150, 255));
        }
    }
    if e.hp < e.max_hp {
        let w = 16.0 * s;
        let y = head.y - 8.0 * s;
        draw_rectangle(
            head.x - w / 2.0 - 1.0,
            y - 1.0,
            w + 2.0,
            5.0,
            rgba(0, 0, 0, 190),
        );
        let f = (e.hp / e.max_hp).clamp(0.0, 1.0);
        let col = if f > 0.5 {
            rgb(120, 220, 90)
        } else if f > 0.25 {
            rgb(240, 200, 60)
        } else {
            rgb(240, 70, 50)
        };
        draw_rectangle(head.x - w / 2.0, y, w * f, 3.0, col);
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
pub fn world(game: &Game, decor: &Decor, time: f32, flash: f32, light: &mut Lighting, vis: Vis) {
    let map = game.map();
    silo(game, decor, time, light, vis);
    it_room(game, time);
    supply_room(game, time);
    // ...and the two key rooms ignore the gloom entirely.
    for tiles in [&sc::map::IT_ROOM[..], &sc::map::SUPPLY_ROOM[..]] {
        let r = room_rect(tiles);
        light.fullbright.push(Rect::new(r.x, r.y, r.w, r.h + 8.0));
    }
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
    pillar(light, vis);
    for (a, b, front) in map.segments() {
        if front {
            stair(a, b, true);
        }
    }
    landing_lamps(game, time, light, vis);
    airlock(game, time, flash);
    for b in game.barriers() {
        barrier_icon(px(b.pos), 1.0, b.level, b.hp / b.max_hp, b.broken, 1.0);
    }
    towers(game);
    for e in enemies.iter().filter(|e| map.is_front(e.dist)) {
        rebel(e, true, time);
    }
    projectiles(game);
    world_lights(game, time, light, vis);
}

/// Caged work lights bolted to the wall at every stair landing.
fn landing_lamps(game: &Game, time: f32, light: &mut Lighting, vis: Vis) {
    let pts: Vec<sc::Vec2> = sc::map::waypoints();
    for (i, w) in pts
        .iter()
        .enumerate()
        .skip(1)
        .take(pts.len().saturating_sub(3))
    {
        let side = if w.x < PILLAR_X { -1.0 } else { 1.0 };
        // A small caged lamp on the shaft wall, just above the landing.
        let p = px(*w) + vec2(side * 20.0, -30.0);
        if !vis.contains(p.y, 60.0) {
            continue;
        }
        draw_line(p.x, p.y - 4.0, p.x, p.y - 8.0, 1.5, rgb(50, 52, 56));
        draw_rectangle(p.x - 3.0, p.y - 4.0, 6.0, 2.0, rgb(60, 62, 64));
        draw_circle(p.x, p.y, 2.6, rgb(255, 214, 140));
        for dx in [-2.5, 0.0, 2.5] {
            draw_line(
                p.x + dx,
                p.y - 2.5,
                p.x + dx * 1.2,
                p.y + 3.0,
                0.7,
                rgb(40, 42, 46),
            );
        }
        let row = (w.y - 0.5) as i32;
        if !game.is_row_dark(row) {
            let hum = 0.92 + 0.08 * (time * 50.0 + i as f32).sin();
            light.add(
                p + vec2(0.0, 30.0),
                140.0,
                Color::new(0.7, 0.82, 1.0, 0.7 * hum),
            );
        }
    }
}

/// Lights from things that move or change: towers, muzzle flashes, projectiles, headlamps.
fn world_lights(game: &Game, time: f32, light: &mut Lighting, vis: Vis) {
    // IT server room glow, or sparks when it's down.
    let it = px(sc::map::it_center());
    if game.cameras_down() {
        if (time * 9.0).sin() > 0.6 {
            light.add_bloom(it, 50.0, Color::new(1.0, 0.8, 0.4, 0.8));
        }
        light.add(it, 90.0, Color::new(0.9, 0.25, 0.15, 0.6));
    } else {
        light.add(it, 120.0, Color::new(0.35, 0.6, 1.0, 0.9));
    }
    let sup = px(sc::map::supply_center());
    let sup_c = if game.supply_joined() {
        Color::new(1.0, 0.35, 0.2, 0.8)
    } else {
        Color::new(1.0, 0.8, 0.5, 0.6)
    };
    light.add(sup, 110.0, sup_c);
    // The airlock: hazard beacon, or daylight pouring in once it's open.
    let al = vec2((AIRLOCK[0].0 as f32 + 1.0) * TILE, HUD_H + TILE * 0.48);
    if game.phase() == Phase::Lost {
        light.add_bloom(al, 160.0, Color::new(1.0, 0.98, 0.9, 1.0));
        light.add(
            al + vec2(0.0, 120.0),
            320.0,
            Color::new(1.0, 0.97, 0.85, 1.0),
        );
    } else {
        let k = 0.5 + 0.5 * (time * 4.0).sin().abs();
        light.add_bloom(al + vec2(70.0, -20.0), 18.0, Color::new(1.0, 0.5, 0.1, k));
    }
    for t in game.towers() {
        let c = px(t.pos);
        if !vis.contains(c.y, 80.0) {
            continue;
        }
        // Each post has a work light so it stays readable in the dark shaft.
        let tc = tower_color(t.kind);
        light.add(c, 72.0, Color::new(0.75, 0.78, 0.85, 0.55));
        light.add(c + vec2(0.0, 18.0), 34.0, Color::new(tc.r, tc.g, tc.b, 0.5));
        let s = t.stats();
        let since = s.cooldown - t.cooldown;
        if t.cooldown > 0.0 && since < 0.08 {
            let col = match t.kind {
                TowerKind::Jammer => Color::new(0.8, 0.5, 1.0, 1.0),
                TowerKind::Gas => Color::new(0.6, 1.0, 0.5, 1.0),
                _ => Color::new(1.0, 0.85, 0.5, 1.0),
            };
            light.add_bloom(c, 34.0, col);
        }
    }
    for p in game.projectiles() {
        let pos = px(p.pos);
        match p.kind {
            sc::ProjectileKind::Bullet => light.add(pos, 16.0, Color::new(1.0, 0.9, 0.6, 0.6)),
            sc::ProjectileKind::Shell => light.add(pos, 26.0, Color::new(0.6, 1.0, 0.45, 0.6)),
        }
    }
    for e in game.enemies() {
        let base = px(e.pos) + vec2(e.lane * TILE * 0.9, -4.0);
        if !vis.contains(base.y, 80.0) {
            continue;
        }
        if e.kind == EnemyKind::Miner {
            // Helmet lamp.
            let head = base - vec2(-5.0, 31.0);
            light.add(
                head + vec2(14.0, 6.0),
                60.0,
                Color::new(1.0, 0.95, 0.75, 0.7),
            );
        }
        if e.kind == EnemyKind::Leader {
            light.add(
                base - vec2(0.0, 30.0),
                70.0,
                Color::new(1.0, 0.4, 0.25, 0.5),
            );
        }
    }
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
        let col = if b.broken { rgb(110, 90, 70) } else { rgb(200, 160, 110) };
        draw_rectangle(p.x - 4.0, p.y - 1.5, 8.0, 3.0, col);
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
