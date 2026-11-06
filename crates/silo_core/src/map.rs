//! The silo, seen in cross-section: a buried concrete cylinder with a spiral staircase winding up
//! a central shaft. Living levels line both sides of the shaft. The airlock sits at the surface.
//!
//! All coordinates are in tile units; tile (c, r) spans [c, c+1) x [r, r+1). Row 0 is the
//! surface, rows 1..ROWS are underground. Columns 0 and COLS-1 are the silo's rock walls.

use crate::math::{Vec2, vec2};
use std::collections::HashSet;

pub const COLS: i32 = 12;
/// Surface row plus 36 underground rows: three sections of 12 rows (48 silo levels) each.
pub const ROWS: i32 = 37;
/// Rows a frontend shows at once: one section plus one row of context above it.
pub const VIEW_ROWS: i32 = 13;
/// Silo levels per map row (36 rows x 4 = 144 levels).
pub const LEVELS_PER_ROW: i32 = 4;

/// The three sections of the silo, top to bottom: (name, first row, last row).
pub const SECTIONS: [(&str, i32, i32); 3] = [("UP TOP", 1, 12), ("MIDS", 13, 24), ("DOWN DEEP", 25, 36)];

/// First row of the viewport that shows section `i` (one row of the section above as context).
pub fn section_view_top(i: usize) -> i32 {
    SECTIONS[i.min(SECTIONS.len() - 1)].1 - 1
}

/// Which section a row belongs to (the surface counts as Up Top).
pub fn section_of_row(r: i32) -> usize {
    SECTIONS.iter().position(|s| r <= s.2).unwrap_or(SECTIONS.len() - 1)
}

/// Silo level printed on the wall at row `r` (1 at the top, 144 at the bottom).
pub fn level_of_row(r: i32) -> u32 {
    ((r.max(1) - 1) * LEVELS_PER_ROW + 1) as u32
}

/// Horizontal extent of the stair shaft (the helix sweeps between these x values).
pub const SHAFT_LEFT: f32 = 3.2;
pub const SHAFT_RIGHT: f32 = 8.8;
/// The central support column the stair winds around.
pub const PILLAR_X: f32 = 6.0;

/// The spiral stair seen side-on: each pass crosses the shaft diagonally, alternating between
/// the front (moving right) and the back (moving left) of the central pillar, climbing two rows
/// per pass. Starts just below the bottom edge (Mechanical) and ends inside the airlock.
pub fn waypoints() -> Vec<Vec2> {
    let mut v = vec![vec2(SHAFT_LEFT, ROWS as f32 + 0.2)];
    let mut y = ROWS - 2;
    let mut right = true;
    while y >= 3 {
        v.push(vec2(if right { SHAFT_RIGHT } else { SHAFT_LEFT }, y as f32));
        right = !right;
        y -= 2;
    }
    v.push(vec2(PILLAR_X, 1.6));
    v.push(vec2(PILLAR_X, 0.5));
    v
}

/// The airlock door spans these two surface tiles.
pub const AIRLOCK: [(i32, i32); 2] = [(5, 0), (6, 0)];

/// IT's server room: two tiles on level ~33, right where a stair landing meets the wall.
/// Rebels passing within `IT_REACH` of its centre attack it.
pub const IT_ROOM: [(i32, i32); 2] = [(1, 9), (2, 9)];
pub const IT_REACH: f32 = 2.0;

pub fn it_center() -> Vec2 {
    vec2(2.0, 9.5)
}

/// The Supply depot in the Down Deep. Once Supply joins the uprising, rebels passing within
/// `SUPPLY_REACH` of it are handed guns and tools. Barriers may only be built above it.
pub const SUPPLY_ROOM: [(i32, i32); 2] = [(9, 26), (10, 26)];
pub const SUPPLY_REACH: f32 = 2.0;

pub fn supply_center() -> Vec2 {
    vec2(10.0, 26.5)
}

/// Named places marked on the right wall (row, name). Level numbers come from `level_of_row`.
pub const PLAQUES: &[(i32, &str)] = &[
    (2, "IT"),
    (6, "CAFE"),
    (10, "SCHOOL"),
    (14, "MARKET"),
    (18, "FARMS"),
    (22, "HYDRO"),
    (26, "SUPPLY"),
    (30, "DEEP"),
    (34, "MECH"),
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
    /// IT's server room. Not buildable; rebels attack it as they pass.
    ItRoom,
    /// Supply's depot. Not buildable; arms the rebels once Supply joins them.
    SupplyRoom,
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
        let points = waypoints();
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
        } else if IT_ROOM.contains(&(c, r)) {
            Tile::ItRoom
        } else if SUPPLY_ROOM.contains(&(c, r)) {
            Tile::SupplyRoom
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

    /// Barriers go on the stair, only above the Supply depot.
    pub fn barrier_allowed(&self, c: i32, r: i32) -> bool {
        self.tile(c, r) == Tile::Stairs && r >= 1 && r < SUPPLY_ROOM[0].1
    }

    /// Where along the path a barrier on stair tile (c, r) stands: the middle of the longest
    /// stretch of stair crossing that tile.
    pub fn barrier_dist(&self, c: i32, r: i32) -> Option<f32> {
        if !self.barrier_allowed(c, r) {
            return None;
        }
        let step = 0.02;
        let mut runs: Vec<(f32, f32)> = vec![];
        let mut d = 0.0;
        while d <= self.total {
            let p = self.pos_at(d);
            if (p.x.floor() as i32, p.y.floor() as i32) == (c, r) {
                match runs.last_mut() {
                    Some(run) if d - run.1 <= step * 1.5 => run.1 = d,
                    _ => runs.push((d, d)),
                }
            }
            d += step;
        }
        runs.into_iter().max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0))).map(|(a, b)| (a + b) / 2.0)
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
        assert!(!m.is_front(m.length() * 0.08));
    }

    #[test]
    fn three_sections_cover_the_silo() {
        assert_eq!(SECTIONS[0].1, 1);
        assert_eq!(SECTIONS[2].2, ROWS - 1);
        for w in SECTIONS.windows(2) {
            assert_eq!(w[0].2 + 1, w[1].1);
        }
        for (i, s) in SECTIONS.iter().enumerate() {
            let top = section_view_top(i);
            assert!(top >= 0 && top + VIEW_ROWS <= ROWS, "section {i} view out of bounds");
            assert!(top + VIEW_ROWS > s.2, "section {i} not fully visible");
        }
        let m = Map::new();
        assert_eq!(m.tile(1, 9), Tile::ItRoom);
        assert!(!m.buildable(2, 9));
        // The stair passes IT's door.
        assert!(m.samples(0.1).any(|p| p.distance(it_center()) < IT_REACH));
        assert_eq!(m.tile(9, 26), Tile::SupplyRoom);
        assert!(m.samples(0.1).any(|p| p.distance(supply_center()) < SUPPLY_REACH));
        // Barriers: stair tiles above Supply only, each mapping to a point on that tile.
        assert!(!m.barrier_allowed(7, 30), "below Supply");
        let mut n = 0;
        for (c, r) in all_tiles() {
            if let Some(d) = m.barrier_dist(c, r) {
                n += 1;
                assert!(r < SUPPLY_ROOM[0].1);
                let p = m.pos_at(d);
                assert_eq!((p.x.floor() as i32, p.y.floor() as i32), (c, r));
            }
        }
        assert!(n > 40, "only {n} barrier spots");
        assert_eq!(section_of_row(0), 0);
        assert_eq!(section_of_row(13), 1);
        assert_eq!(section_of_row(36), 2);
        assert_eq!(level_of_row(1), 1);
        assert_eq!(level_of_row(36), 141);
        // The stair climbs through every section.
        for (_, a, b) in SECTIONS {
            assert!((a..=b).any(|r| (3..=8).any(|c| m.tile(c, r) == Tile::Stairs)));
        }
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
        assert!(free >= 150, "only {free} buildable tiles");
    }

    #[test]
    fn levels_descend_from_mechanical_to_airlock() {
        let m = Map::new();
        assert_eq!(m.level_at(0.0), 144);
        assert_eq!(m.level_at(m.length()), 1);
    }
}
