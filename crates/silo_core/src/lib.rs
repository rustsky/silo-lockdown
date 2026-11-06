//! # Silo: Lockdown — core engine
//!
//! You are IT. The uprising climbs the stairwell from Mechanical toward the airlock.
//! Place security towers on the landings; if the airlock seals fail, the silo is opened.
//!
//! The engine is pure Rust with no dependencies and no rendering. It is deterministic:
//! the same seed and the same commands at the same ticks always produce the same game.
//! Coordinates are in tiles; the grid is [`map::COLS`] x [`map::ROWS`].

pub mod bot;
pub mod game;
pub mod map;
pub mod math;
pub mod rng;
pub mod stats;

pub use game::{Command, CommandError, Enemy, Event, Game, Id, Phase, Projectile, ProjectileKind, TICK, Tower};
pub use map::{Map, Tile};
pub use math::{Vec2, vec2};
pub use stats::{EnemyKind, TowerKind};
