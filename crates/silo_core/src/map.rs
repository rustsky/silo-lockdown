//! The silo, seen in cross-section: a buried concrete cylinder with a spiral staircase winding up
//! a central shaft. Living levels line both sides of the shaft. The airlock sits at the surface.
//!
//! All coordinates are in tile units; tile (c, r) spans [c, c+1) x [r, r+1). Row 0 is the
//! surface, rows 1..ROWS are underground. Columns 0 and COLS-1 are the silo's rock walls.

use crate::math::{Vec2, vec2};
use std::collections::HashSet;

pub const COLS: i32 = 12;
pub const ROWS: i32 = 13;

/// Horizontal extent of the stair shaft (the helix sweeps between these x values).
pub const SHAFT_LEFT: f32 = 3.2;
pub const SHAFT_RIGHT: f32 = 8.8;
/// The central support column the stair winds around.
pub const PILLAR_X: f32 = 6.0;

/// The spiral stair seen side-on: each pass crosses the shaft diagonally, alternating between
/// the front (moving right) and the back (moving left) of the central pillar.
/// Starts just below the bottom edge (Mechanical) and ends inside the airlock.
pub const WAYPOINTS: &[(f32, f32)] = &[
    (SHAFT_LEFT, 13.2),
    (SHAFT_RIGHT, 11.0),
    (SHAFT_LEFT, 9.0),
    (SHAFT_RIGHT, 7.0),
    (SHAFT_LEFT, 5.0),
    (SHAFT_RIGHT, 3.0),
    (PILLAR_X, 1.6),
    (PILLAR_X, 0.5),
];

/// The airlock door spans these two surface tiles.
pub const AIRLOCK: [(i32, i32); 2] = [(5, 0), (6, 0)];

/// Level markers carved into the left wall (row, silo level) and section names on the right.
pub const PLAQUES: &[(i32, u32, &str)] = &[
    (12, 142, "MECHANICAL"),
    (10, 118, "DOWN DEEP"),
    (8, 92, "SUPPLY"),
    (6, 70, "MIDS"),
    (4, 48, "UP TOP"),
    (2, 34, "IT"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    /// Above ground. Not buildable.
    Outside,
    Airlock,
    /// Silo wall / bedrock. Not buildable.
    Rock,
    /// The spiral stair passes through this tile. Not buildable.
    Stairs,
    /// Open stairwell between passes of the spiral. Buildable (catwalk platforms).
    Shaft,
    /// A living level beside the shaft. Buildable.
    Floor,
    OffMap,
}

#[derive(Clone, Debug)]
pub struct Map {
    points: Vec<Vec2>,
    cum: Vec<f32>,
    total: f32,
    stairs: HashSet<(i32, i32)>,
}

pub fn tile_center(c: i32, r: i32) -> Vec2 {
    vec2(c as f32 + 0.5, r as f32 + 0.5)
}

pub fn in_bounds(c: i32, r: i32) -> bool {
    (0..COLS).contains(&c) && (0..ROWS).contains(&r)
}

impl Default for Map {
    fn default() -> Self {
        Self::new()
    }
}

impl Map {
    pub fn new() -> Self {
        let points: Vec<Vec2> = WAYPOINTS.iter().map(|&(x, y)| vec2(x, y)).collect();
        let mut cum = vec![0.0];
        for w in points.windows(2) {
            let last = *cum.last().unwrap();
            cum.push(last + w[0].distance(w[1]));
        }
        let total = *cum.last().unwrap();
        let mut map = Self { points, cum, total, stairs: HashSet::new() };

        // Every tile the stair passes through is blocked.
        let mut d = 0.0;
        while d <= total {
            let p = map.pos_at(d);
            let t = (p.x.floor() as i32, p.y.floor() as i32);
            if in_bounds(t.0, t.1) {
                map.stairs.insert(t);
            }
            d += 0.02;
        }
        map
    }

    /// Total path length in tiles.
    pub fn length(&self) -> f32 {
        self.total
    }

    fn segment(&self, d: f32) -> usize {
        let d = d.clamp(0.0, self.total);
        (0..self.points.len() - 1).find(|&i| d <= self.cum[i + 1]).unwrap_or(self.points.len() - 2)
    }

    /// Position after travelling `d` tiles along the path (clamped to the ends).
    pub fn pos_at(&self, d: f32) -> Vec2 {
        let d = d.clamp(0.0, self.total);
        let i = self.segment(d);
        let seg = self.cum[i + 1] - self.cum[i];
        let t = if seg > 0.0 { (d - self.cum[i]) / seg } else { 0.0 };
        self.points[i].lerp(self.points[i + 1], t)
    }

    /// True on passes in front of the central pillar (renderers draw these on top).
    pub fn is_front(&self, d: f32) -> bool {
        let i = self.segment(d);
        self.points[i + 1].x > self.points[i].x
    }

    /// The path as line segments (a, b, is_front), for renderers.
    pub fn segments(&self) -> impl Iterator<Item = (Vec2, Vec2, bool)> + '_ {
        self.points.windows(2).map(|w| (w[0], w[1], w[1].x > w[0].x))
    }

    /// Silo level (144 at the bottom .. 1 at the airlock) for a distance along the path.
    pub fn level_at(&self, d: f32) -> u32 {
        let t = (d / self.total).clamp(0.0, 1.0);
        (144.0 - t * 143.0).round() as u32
    }

    pub fn tile(&self, c: i32, r: i32) -> Tile {
        if !in_bounds(c, r) {
            Tile::OffMap
        } else if AIRLOCK.contains(&(c, r)) {
            Tile::Airlock
        } else if r == 0 {
            Tile::Outside
        } else if c == 0 || c == COLS - 1 || (r == 1 && (c == 1 || c == COLS - 2)) {
            // Walls, plus the rounded shoulders of the silo's dome.
            Tile::Rock
        } else if self.stairs.contains(&(c, r)) {
            Tile::Stairs
        } else if (c as f32) >= SHAFT_LEFT.floor() && ((c + 1) as f32) <= SHAFT_RIGHT.ceil() {
            Tile::Shaft
        } else {
            Tile::Floor
        }
    }

    pub fn buildable(&self, c: i32, r: i32) -> bool {
        matches!(self.tile(c, r), Tile::Floor | Tile::Shaft)
    }

    /// Evenly spaced samples along the path (useful for AI/bot coverage scoring).
    pub fn samples(&self, step: f32) -> impl Iterator<Item = Vec2> + '_ {
        let n = (self.total / step).ceil() as usize;
        (0..=n).map(move |i| self.pos_at(i as f32 * step))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_tiles() -> impl Iterator<Item = (i32, i32)> {
        (0..COLS).flat_map(|c| (0..ROWS).map(move |r| (c, r)))
    }

    #[test]
    fn path_only_crosses_stairs_and_reaches_airlock() {
        let m = Map::new();
        let mut d = 0.0;
        while d <= m.length() {
            let p = m.pos_at(d);
            let (c, r) = (p.x.floor() as i32, p.y.floor() as i32);
            if in_bounds(c, r) {
                assert!(
                    matches!(m.tile(c, r), Tile::Stairs | Tile::Airlock),
                    "sample at d={d} landed on {:?} at ({c},{r})",
                    m.tile(c, r)
                );
            }
            d += 0.01;
        }
        let end = m.pos_at(m.length());
        assert!(AIRLOCK.contains(&(end.x.floor() as i32, end.y.floor() as i32)));
        assert!(m.pos_at(0.0).y >= ROWS as f32, "path must start below the map");
    }

    #[test]
    fn pos_at_is_continuous_and_clamped() {
        let m = Map::new();
        let mut prev = m.pos_at(0.0);
        let mut d = 0.0;
        while d < m.length() {
            d += 0.01;
            let p = m.pos_at(d);
            assert!(p.distance(prev) <= 0.0101, "jump at d={d}");
            prev = p;
        }
        assert_eq!(m.pos_at(-5.0), m.pos_at(0.0));
        assert_eq!(m.pos_at(1e6), m.pos_at(m.length()));
    }

    #[test]
    fn helix_alternates_front_and_back() {
        let m = Map::new();
        let fronts: Vec<bool> = m.segments().map(|s| s.2).collect();
        assert!(fronts[0], "first pass should be in front");
        for w in fronts[..5].windows(2) {
            assert_ne!(w[0], w[1], "passes must alternate");
        }
        assert!(m.is_front(0.1));
        assert!(!m.is_front(m.length() * 0.25));
    }

    #[test]
    fn stair_climbs_monotonically() {
        let m = Map::new();
        for (a, b, _) in m.segments() {
            assert!(b.y < a.y, "every pass must go up");
        }
    }

    #[test]
    fn tile_classification() {
        let m = Map::new();
        assert_eq!(m.tile(5, 0), Tile::Airlock);
        assert_eq!(m.tile(6, 0), Tile::Airlock);
        assert_eq!(m.tile(2, 0), Tile::Outside);
        assert_eq!(m.tile(0, 5), Tile::Rock);
        assert_eq!(m.tile(COLS - 1, 5), Tile::Rock);
        assert_eq!(m.tile(1, 1), Tile::Rock);
        assert_eq!(m.tile(1, 5), Tile::Floor);
        assert_eq!(m.tile(10, 5), Tile::Floor);
        assert_eq!(m.tile(-1, 5), Tile::OffMap);
        // Gaps between spiral passes in the middle of the shaft are catwalks.
        assert_eq!(m.tile(6, 9), Tile::Shaft);
        assert!(m.buildable(6, 9));
        for (c, r) in all_tiles() {
            let t = m.tile(c, r);
            assert_eq!(m.buildable(c, r), matches!(t, Tile::Floor | Tile::Shaft));
            if t == Tile::Stairs {
                assert!((3..=8).contains(&c), "stairs outside shaft at ({c},{r})");
            }
        }
        let free = all_tiles().filter(|&(c, r)| m.buildable(c, r)).count();
        assert!(free >= 50, "only {free} buildable tiles");
    }

    #[test]
    fn levels_descend_from_mechanical_to_airlock() {
        let m = Map::new();
        assert_eq!(m.level_at(0.0), 144);
        assert_eq!(m.level_at(m.length()), 1);
    }
}
