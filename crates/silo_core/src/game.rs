//! Deterministic fixed-tick simulation. Frontends call `step()` at `TICK` intervals (or several
//! times per frame for fast-forward), issue commands, and render from the read-only accessors and
//! the event stream.

use crate::map::{
    IT_REACH, Map, ROWS, SECTIONS, SUPPLY_REACH, in_bounds, it_center, supply_center,
};
use crate::math::Vec2;
use crate::rng::Rng;
use crate::stats::*;
use std::collections::VecDeque;

/// Simulation step in seconds (30 Hz).
pub const TICK: f32 = 1.0 / 30.0;
const MAX_BUFFERED_EVENTS: usize = 4096;
/// Rebels pile up this far (tiles) below a barrier.
const BARRIER_STOP: f32 = 0.25;

pub type Id = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Between waves: build freely, then send the next wave.
    Building,
    Wave,
    /// All waves survived.
    Won,
    /// The airlock opened.
    Lost,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tower {
    pub id: Id,
    pub kind: TowerKind,
    pub col: i32,
    pub row: i32,
    pub pos: Vec2,
    pub level: u8,
    /// Total chits spent on this tower (purchase + upgrades).
    pub invested: u32,
    /// Seconds until the tower may fire again.
    pub cooldown: f32,
    /// Last aim point, for renderers.
    pub aim: Option<Vec2>,
    pub kills: u32,
    /// Armed rebels shoot towers; at 0 the tower is destroyed. Repaired between waves.
    pub hp: f32,
    pub max_hp: f32,
    /// How far it actually hits from where it stands: its range, pushed out into the
    /// stairwell where walls and floors would waste it (see `map::reach`).
    pub reach: f32,
}

impl Tower {
    pub fn stats(&self) -> TowerStats {
        self.kind.stats(self.level)
    }
    pub fn upgrade_cost(&self) -> Option<u32> {
        self.kind.upgrade_cost(self.level)
    }
    pub fn sell_value(&self) -> u32 {
        (self.invested as f32 * SELL_REFUND).floor() as u32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Enemy {
    pub id: Id,
    pub kind: EnemyKind,
    pub hp: f32,
    pub max_hp: f32,
    /// Tiles travelled along the path.
    pub dist: f32,
    pub pos: Vec2,
    /// Base speed in tiles/second.
    pub speed: f32,
    pub armor: f32,
    pub bounty: u32,
    pub breach: u32,
    /// Current slow fraction (0..1) and remaining duration.
    pub slow: f32,
    pub slow_time: f32,
    /// Cosmetic sideways offset (-0.2..0.2) so crowds don't overlap. Not used by logic.
    pub lane: f32,
    /// Handed a rifle by Supply: shoots towers in passing.
    pub armed: bool,
    /// Handed breaching tools by Supply: smashes barriers quickly.
    pub tools: bool,
    /// Already went past the Supply depot.
    pub supplied: bool,
    /// Stopped at a barrier, beating on it.
    pub blocked: bool,
    /// Seconds until an armed rebel may fire again.
    pub gun_cooldown: f32,
    last_hit_by: Option<Id>,
}

impl Enemy {
    /// A stand-in rebel for renderers (cut-scenes, portraits). Not part of any game.
    pub fn for_display(kind: EnemyKind, pos: Vec2) -> Self {
        let s = kind.base();
        Self {
            id: 0,
            kind,
            hp: s.hp,
            max_hp: s.hp,
            dist: 0.0,
            pos,
            speed: s.speed,
            armor: s.armor,
            bounty: 0,
            breach: 0,
            slow: 0.0,
            slow_time: 0.0,
            lane: 0.0,
            armed: false,
            tools: false,
            supplied: false,
            blocked: true,
            gun_cooldown: 0.0,
            last_hit_by: None,
        }
    }

    pub fn effective_speed(&self) -> f32 {
        self.speed * (1.0 - self.slow)
    }
    pub fn is_slowed(&self) -> bool {
        self.slow_time > 0.0 && self.slow > 0.0
    }
}

/// Barricade across the stair. Rebels can't pass until they break it.
#[derive(Clone, Debug, PartialEq)]
pub struct Barrier {
    pub id: Id,
    pub col: i32,
    pub row: i32,
    /// Distance along the path where it stands.
    pub dist: f32,
    pub pos: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub level: u8,
    /// Chits spent on it (build + upgrades).
    pub invested: u32,
    /// Smashed: a wreck that no longer blocks. IT rebuilds it after the wave.
    pub broken: bool,
}

impl Barrier {
    /// Refund for tearing it down: scales with remaining strength (nothing for a wreck).
    pub fn refund(&self) -> u32 {
        if self.broken {
            return 0;
        }
        (self.invested as f32 * SELL_REFUND * self.hp / self.max_hp).floor() as u32
    }
    pub fn upgrade_cost(&self) -> Option<u32> {
        barrier_upgrade_cost(self.level)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileKind {
    /// Homing bullet; fizzles if its target dies first.
    Bullet,
    /// Lobbed at a point; splashes on landing.
    Shell,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Projectile {
    pub id: Id,
    pub kind: ProjectileKind,
    pub tower: Id,
    pub pos: Vec2,
    pub dest: Vec2,
    pub target: Option<Id>,
    speed: f32,
    damage: f32,
    splash: f32,
    pierce: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Place { kind: TowerKind, col: i32, row: i32 },
    Upgrade { tower: Id },
    Sell { tower: Id },
    PlaceBarrier { col: i32, row: i32 },
    RemoveBarrier { barrier: Id },
    UpgradeBarrier { barrier: Id },
    /// Pay Judicial to arrest a rebel on the stair.
    Arrest { enemy: Id },
    SendWave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    GameOver,
    OutOfBounds,
    NotBuildable,
    Occupied,
    NotEnoughChits { need: u32, have: u32 },
    NoSuchTower,
    MaxLevel,
    WaveInProgress,
    /// Barriers only go on the stair above the Supply depot.
    BarrierNotAllowed,
    NoSuchBarrier,
    /// This kind of tower doesn't go on this kind of ground (see `Map::tower_allowed`).
    WrongGround { kind: TowerKind },
    /// Even stretched across the stairwell, its range never touches the stair from there.
    OutOfReach,
    /// The barrier is a wreck until the wave ends.
    BarrierBroken,
    NoSuchEnemy,
    /// Judicial can't arrest someone the cameras can't see.
    NotVisible,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GameOver => write!(f, "the game is over"),
            Self::OutOfBounds => write!(f, "outside the silo"),
            Self::NotBuildable => write!(f, "can't build there"),
            Self::Occupied => write!(f, "tile already occupied"),
            Self::NotEnoughChits { need, have } => write!(f, "need {need} chits (have {have})"),
            Self::NoSuchTower => write!(f, "no tower there"),
            Self::MaxLevel => write!(f, "already max level"),
            Self::WaveInProgress => write!(f, "wave already in progress"),
            Self::BarrierNotAllowed => write!(f, "barriers go on the stair above Supply"),
            Self::NoSuchBarrier => write!(f, "no barrier there"),
            Self::WrongGround { kind } => write!(f, "{} goes on {}", kind.name(), kind.ground()),
            Self::OutOfReach => write!(f, "out of reach: rebels never pass within range here"),
            Self::BarrierBroken => write!(f, "that barrier is wrecked; IT rebuilds it after the wave"),
            Self::NoSuchEnemy => write!(f, "no rebel there"),
            Self::NotVisible => write!(f, "the cameras can't see them"),
        }
    }
}

impl std::error::Error for CommandError {}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    WaveStarted { wave: u32, lore: &'static str },
    WaveCleared { wave: u32, bonus: u32 },
    EnemySpawned { id: Id, kind: EnemyKind },
    /// `dist` is how far along the path the rebel got.
    EnemyKilled { id: Id, kind: EnemyKind, pos: Vec2, dist: f32, bounty: u32 },
    Breach { id: Id, kind: EnemyKind, seals_lost: u32 },
    TowerPlaced { id: Id, kind: TowerKind, col: i32, row: i32 },
    TowerUpgraded { id: Id, level: u8 },
    TowerSold { id: Id, refund: u32 },
    /// A tower fired (bullet/shell launch, or marksman hitscan from->to).
    Shot { tower: Id, kind: TowerKind, from: Vec2, to: Vec2 },
    Pulse { tower: Id, pos: Vec2, radius: f32 },
    Explosion { pos: Vec2, radius: f32 },
    /// An armed rebel fired at a tower.
    EnemyShot { enemy: Id, tower: Id, from: Vec2, to: Vec2 },
    TowerDestroyed { id: Id, kind: TowerKind, pos: Vec2 },
    /// Supply has joined the uprising (start of wave `SUPPLY_JOINS_WAVE`).
    SupplyJoined,
    /// A rebel picked up equipment at the Supply depot.
    Equipped { id: Id, pos: Vec2, gun: bool, tools: bool },
    BarrierPlaced { id: Id, col: i32, row: i32 },
    BarrierBroken { id: Id, pos: Vec2 },
    BarrierRemoved { id: Id, refund: u32 },
    BarrierUpgraded { id: Id, level: u8 },
    /// End of wave: IT rebuilt `rebuilt` wrecked barriers and patched the damaged ones.
    BarriersRepaired { rebuilt: u32 },
    /// Judicial arrested a rebel. A leader is sent to cleaning.
    Arrested { id: Id, kind: EnemyKind, pos: Vec2, dist: f32, cost: u32 },
    /// IT's server room fell: cameras below Up Top are dark for `seconds`.
    ItCollapsed { seconds: f32 },
    ItRestored,
    /// Main power cut: a whole section (rows `first..=last`) is dark for `seconds`.
    PowerCut { first: i32, last: i32, seconds: f32 },
    Won,
    Lost,
}

/// A band of floors with no power: frontends must not show what happens there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blackout {
    pub first: i32,
    pub last: i32,
    pub time_left: f32,
}

/// Running totals, mostly for end screens and for accounting tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub kills: u32,
    pub breaches: u32,
    pub towers_lost: u32,
    pub barriers_broken: u32,
    /// Rebels arrested by Judicial.
    pub arrests: u32,
    /// Rebels Supply handed guns / tools to.
    pub armed: u32,
    pub tooled: u32,
    pub chits_earned: u32,
    pub chits_spent: u32,
    pub chits_refunded: u32,
}

#[derive(Clone, Debug)]
pub struct Game {
    map: Map,
    rng: Rng,
    phase: Phase,
    chits: u32,
    seals: u32,
    wave: u32,
    tick: u64,
    next_id: Id,
    towers: Vec<Tower>,
    barriers: Vec<Barrier>,
    enemies: Vec<Enemy>,
    projectiles: Vec<Projectile>,
    spawn_queue: VecDeque<Spawn>,
    spawn_timer: f32,
    events: Vec<Event>,
    totals: Totals,
    it_hp: f32,
    /// Seconds until IT is back online (0 = online).
    it_down: f32,
    power_timer: f32,
    blackouts: Vec<Blackout>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            map: Map::new(),
            rng: Rng::new(seed),
            phase: Phase::Building,
            chits: START_CHITS,
            seals: START_SEALS,
            wave: 0,
            tick: 0,
            next_id: 1,
            towers: Vec::new(),
            barriers: Vec::new(),
            enemies: Vec::new(),
            projectiles: Vec::new(),
            spawn_queue: VecDeque::new(),
            spawn_timer: 0.0,
            events: Vec::new(),
            totals: Totals::default(),
            it_hp: IT_HP,
            it_down: 0.0,
            power_timer: 0.0,
            blackouts: Vec::new(),
        }
    }

    // ------------------------------------------------------------ accessors

    pub fn map(&self) -> &Map {
        &self.map
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Won | Phase::Lost)
    }
    pub fn chits(&self) -> u32 {
        self.chits
    }
    pub fn seals(&self) -> u32 {
        self.seals
    }
    /// Number of the current (or last started) wave; 0 before the first.
    pub fn wave(&self) -> u32 {
        self.wave
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn time(&self) -> f32 {
        self.tick as f32 * TICK
    }
    pub fn towers(&self) -> &[Tower] {
        &self.towers
    }
    pub fn barriers(&self) -> &[Barrier] {
        &self.barriers
    }
    pub fn barrier(&self, id: Id) -> Option<&Barrier> {
        self.barriers.iter().find(|b| b.id == id)
    }
    pub fn barrier_at(&self, col: i32, row: i32) -> Option<&Barrier> {
        self.barriers.iter().find(|b| b.col == col && b.row == row)
    }
    /// True once Supply hands out guns and tools.
    pub fn supply_joined(&self) -> bool {
        self.wave >= SUPPLY_JOINS_WAVE
    }
    pub fn can_place_barrier(&self, col: i32, row: i32) -> Result<(), CommandError> {
        if self.is_over() {
            return Err(CommandError::GameOver);
        }
        if !in_bounds(col, row) {
            return Err(CommandError::OutOfBounds);
        }
        if !self.map.barrier_allowed(col, row) {
            return Err(CommandError::BarrierNotAllowed);
        }
        if self.barrier_at(col, row).is_some() || self.tower_at(col, row).is_some() {
            return Err(CommandError::Occupied);
        }
        self.afford(BARRIER_COST)
    }
    pub fn enemies(&self) -> &[Enemy] {
        &self.enemies
    }
    pub fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }
    pub fn totals(&self) -> &Totals {
        &self.totals
    }
    pub fn it_hp(&self) -> f32 {
        self.it_hp
    }
    /// Seconds until IT's server room is back online; 0 while it stands.
    pub fn it_down(&self) -> f32 {
        self.it_down
    }
    /// While IT is down, frontends must not show the Mids or the Down Deep.
    pub fn cameras_down(&self) -> bool {
        self.it_down > 0.0
    }
    pub fn blackouts(&self) -> &[Blackout] {
        &self.blackouts
    }
    /// True if map row `r` is hidden from the player (power cut, or no cameras there).
    pub fn is_row_dark(&self, r: i32) -> bool {
        (self.cameras_down() && r > SECTIONS[0].2)
            || self.blackouts.iter().any(|b| (b.first..=b.last).contains(&r))
    }
    /// True if the player can currently see a world position.
    pub fn is_visible(&self, p: Vec2) -> bool {
        !self.is_row_dark(p.y.floor() as i32)
    }

    /// Enemies still waiting to enter the stairwell this wave.
    pub fn pending_spawns(&self) -> usize {
        self.spawn_queue.len()
    }
    pub fn tower(&self, id: Id) -> Option<&Tower> {
        self.towers.iter().find(|t| t.id == id)
    }
    pub fn tower_at(&self, col: i32, row: i32) -> Option<&Tower> {
        self.towers.iter().find(|t| t.col == col && t.row == row)
    }
    pub fn can_place(&self, kind: TowerKind, col: i32, row: i32) -> Result<(), CommandError> {
        if self.is_over() {
            return Err(CommandError::GameOver);
        }
        if !in_bounds(col, row) {
            return Err(CommandError::OutOfBounds);
        }
        if !self.map.tower_allowed(kind, col, row) {
            return Err(if self.map.buildable(col, row) {
                CommandError::WrongGround { kind }
            } else {
                CommandError::NotBuildable
            });
        }
        if self.tower_at(col, row).is_some() || self.barrier_at(col, row).is_some() {
            return Err(CommandError::Occupied);
        }
        if !self.map.in_reach(col, row, kind.stats(1).range) {
            return Err(CommandError::OutOfReach);
        }
        self.afford(kind.cost())
    }

    /// Take buffered events. Call once per frame.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    // ------------------------------------------------------------ commands

    pub fn apply(&mut self, cmd: Command) -> Result<(), CommandError> {
        match cmd {
            Command::Place { kind, col, row } => self.place(kind, col, row).map(|_| ()),
            Command::Upgrade { tower } => self.upgrade(tower).map(|_| ()),
            Command::Sell { tower } => self.sell(tower).map(|_| ()),
            Command::PlaceBarrier { col, row } => self.place_barrier(col, row).map(|_| ()),
            Command::RemoveBarrier { barrier } => self.remove_barrier(barrier).map(|_| ()),
            Command::UpgradeBarrier { barrier } => self.upgrade_barrier(barrier).map(|_| ()),
            Command::Arrest { enemy } => self.arrest(enemy).map(|_| ()),
            Command::SendWave => self.send_wave().map(|_| ()),
        }
    }

    pub fn place(&mut self, kind: TowerKind, col: i32, row: i32) -> Result<Id, CommandError> {
        self.can_place(kind, col, row)?;
        let cost = kind.cost();
        self.spend(cost);
        let id = self.alloc_id();
        self.towers.push(Tower {
            id,
            kind,
            col,
            row,
            pos: self.map.tower_pos(col, row),
            level: 1,
            invested: cost,
            cooldown: 0.0,
            aim: None,
            kills: 0,
            hp: kind.max_hp(1),
            max_hp: kind.max_hp(1),
            reach: crate::map::reach(self.map.tower_pos(col, row), kind.stats(1).range),
        });
        self.emit(Event::TowerPlaced { id, kind, col, row });
        Ok(id)
    }

    /// Returns the new level.
    pub fn upgrade(&mut self, id: Id) -> Result<u8, CommandError> {
        self.check_live()?;
        let t = self.tower(id).ok_or(CommandError::NoSuchTower)?;
        let cost = t.upgrade_cost().ok_or(CommandError::MaxLevel)?;
        self.afford(cost)?;
        self.spend(cost);
        let t = self.towers.iter_mut().find(|t| t.id == id).unwrap();
        t.level += 1;
        t.invested += cost;
        let new_max = t.kind.max_hp(t.level);
        t.hp += new_max - t.max_hp;
        t.max_hp = new_max;
        t.reach = crate::map::reach(t.pos, t.stats().range);
        let level = t.level;
        self.emit(Event::TowerUpgraded { id, level });
        Ok(level)
    }

    /// Returns the refund.
    pub fn sell(&mut self, id: Id) -> Result<u32, CommandError> {
        self.check_live()?;
        let idx = self.towers.iter().position(|t| t.id == id).ok_or(CommandError::NoSuchTower)?;
        let refund = self.towers.remove(idx).sell_value();
        self.chits += refund;
        self.totals.chits_refunded += refund;
        self.emit(Event::TowerSold { id, refund });
        Ok(refund)
    }

    pub fn place_barrier(&mut self, col: i32, row: i32) -> Result<Id, CommandError> {
        self.can_place_barrier(col, row)?;
        let dist = self.map.barrier_dist(col, row).ok_or(CommandError::BarrierNotAllowed)?;
        self.spend(BARRIER_COST);
        let id = self.alloc_id();
        self.barriers.push(Barrier {
            id,
            col,
            row,
            dist,
            pos: self.map.pos_at(dist),
            hp: barrier_hp(1),
            max_hp: barrier_hp(1),
            level: 1,
            invested: BARRIER_COST,
            broken: false,
        });
        self.emit(Event::BarrierPlaced { id, col, row });
        Ok(id)
    }

    /// Tear a barrier down. Refund scales with its remaining strength.
    pub fn remove_barrier(&mut self, id: Id) -> Result<u32, CommandError> {
        self.check_live()?;
        let idx = self.barriers.iter().position(|b| b.id == id).ok_or(CommandError::NoSuchBarrier)?;
        let refund = self.barriers.remove(idx).refund();
        self.chits += refund;
        self.totals.chits_refunded += refund;
        self.emit(Event::BarrierRemoved { id, refund });
        Ok(refund)
    }

    /// Reinforce a barrier. Returns the new level.
    pub fn upgrade_barrier(&mut self, id: Id) -> Result<u8, CommandError> {
        self.check_live()?;
        let b = self.barrier(id).ok_or(CommandError::NoSuchBarrier)?;
        if b.broken {
            return Err(CommandError::BarrierBroken);
        }
        let cost = b.upgrade_cost().ok_or(CommandError::MaxLevel)?;
        self.afford(cost)?;
        self.spend(cost);
        let b = self.barriers.iter_mut().find(|b| b.id == id).unwrap();
        b.level += 1;
        b.invested += cost;
        let new_max = barrier_hp(b.level);
        b.hp += new_max - b.max_hp;
        b.max_hp = new_max;
        let level = b.level;
        self.emit(Event::BarrierUpgraded { id, level });
        Ok(level)
    }

    /// What Judicial charges to arrest this rebel right now.
    pub fn arrest_cost(&self, enemy: &Enemy) -> u32 {
        arrest_cost(enemy.kind, self.wave)
    }

    /// Pay Judicial to arrest a rebel on the stair. No bounty is paid for arrests.
    /// Returns the cost.
    pub fn arrest(&mut self, id: Id) -> Result<u32, CommandError> {
        self.check_live()?;
        let idx = self
            .enemies
            .iter()
            .position(|e| e.id == id && e.hp > 0.0)
            .ok_or(CommandError::NoSuchEnemy)?;
        if !self.is_visible(self.enemies[idx].pos) {
            return Err(CommandError::NotVisible);
        }
        let cost = self.arrest_cost(&self.enemies[idx]);
        self.afford(cost)?;
        self.spend(cost);
        let e = self.enemies.remove(idx);
        self.totals.arrests += 1;
        self.emit(Event::Arrested { id: e.id, kind: e.kind, pos: e.pos, dist: e.dist, cost });
        Ok(cost)
    }

    /// Start the next wave. Returns its number.
    pub fn send_wave(&mut self) -> Result<u32, CommandError> {
        self.check_live()?;
        if self.phase == Phase::Wave {
            return Err(CommandError::WaveInProgress);
        }
        self.wave += 1;
        self.phase = Phase::Wave;
        self.spawn_queue = wave(self.wave).into();
        // The uprising gathers at the generator hall before anyone climbs.
        self.spawn_timer = RALLY_TIME + self.spawn_queue.front().map_or(0.0, |s| s.delay);
        self.power_timer = self.rng.range_f32(4.0, 10.0);
        self.emit(Event::WaveStarted { wave: self.wave, lore: wave_lore(self.wave) });
        if self.wave == SUPPLY_JOINS_WAVE {
            self.emit(Event::SupplyJoined);
        }
        Ok(self.wave)
    }

    // ------------------------------------------------------------ simulation

    /// Advance one fixed tick. No-op outside of a wave.
    pub fn step(&mut self) {
        if self.phase != Phase::Wave {
            return;
        }
        self.tick += 1;
        self.spawn();
        self.move_enemies();
        if self.resolve_breaches() {
            return;
        }
        self.fire_towers();
        self.rebels_act();
        self.move_projectiles();
        self.collect_dead();
        self.update_power();
        self.check_wave_end();
    }

    /// Step up to `max_ticks` until the current wave ends (or the game ends). Returns ticks run.
    pub fn run_wave(&mut self, max_ticks: u64) -> u64 {
        let mut n = 0;
        while self.phase == Phase::Wave && n < max_ticks {
            self.step();
            n += 1;
        }
        n
    }

    fn spawn(&mut self) {
        self.spawn_timer -= TICK;
        while self.spawn_timer <= 0.0 {
            let Some(s) = self.spawn_queue.pop_front() else { break };
            self.spawn_enemy(s.kind);
            match self.spawn_queue.front() {
                Some(next) => self.spawn_timer += next.delay,
                None => break,
            }
        }
    }

    fn spawn_enemy(&mut self, kind: EnemyKind) {
        let s = kind.for_wave(self.wave);
        let id = self.alloc_id();
        let lane = self.rng.range_f32(-0.2, 0.2);
        self.enemies.push(Enemy {
            id,
            kind,
            hp: s.hp,
            max_hp: s.hp,
            dist: 0.0,
            pos: self.map.pos_at(0.0),
            speed: s.speed,
            armor: s.armor,
            bounty: s.bounty,
            breach: s.breach,
            slow: 0.0,
            slow_time: 0.0,
            lane,
            armed: false,
            tools: false,
            supplied: false,
            blocked: false,
            gun_cooldown: 0.5,
            last_hit_by: None,
        });
        self.emit(Event::EnemySpawned { id, kind });
    }

    fn move_enemies(&mut self) {
        let len = self.map.length();
        for e in &mut self.enemies {
            if e.slow_time > 0.0 {
                e.slow_time -= TICK;
                if e.slow_time <= 0.0 {
                    e.slow_time = 0.0;
                    e.slow = 0.0;
                }
            }
            let mut next = (e.dist + e.effective_speed() * TICK).min(len);
            // The first intact barrier ahead stops the climb.
            e.blocked = false;
            if let Some(b) = self
                .barriers
                .iter()
                .filter(|b| !b.broken && b.dist > e.dist)
                .min_by(|a, b| a.dist.total_cmp(&b.dist))
                && next >= b.dist - BARRIER_STOP
            {
                next = e.dist.max(b.dist - BARRIER_STOP);
                e.blocked = true;
            }
            e.dist = next;
            e.pos = self.map.pos_at(e.dist);
        }
    }

    /// Returns true if the game was lost.
    fn resolve_breaches(&mut self) -> bool {
        let len = self.map.length();
        let mut i = 0;
        while i < self.enemies.len() {
            if self.enemies[i].dist >= len {
                let e = self.enemies.swap_remove(i);
                self.seals = self.seals.saturating_sub(e.breach);
                self.totals.breaches += 1;
                self.emit(Event::Breach { id: e.id, kind: e.kind, seals_lost: e.breach });
            } else {
                i += 1;
            }
        }
        // Keep enemies ordered by id so iteration (and thus the sim) stays deterministic
        // regardless of removal order.
        self.enemies.sort_unstable_by_key(|e| e.id);
        if self.seals == 0 {
            self.phase = Phase::Lost;
            self.spawn_queue.clear();
            self.projectiles.clear();
            self.emit(Event::Lost);
            return true;
        }
        false
    }

    fn fire_towers(&mut self) {
        for ti in 0..self.towers.len() {
            let t = &mut self.towers[ti];
            t.cooldown = (t.cooldown - TICK).max(0.0);
            if t.cooldown > 0.0 {
                continue;
            }
            let st = t.stats();
            let (tid, tkind, tpos, reach) = (t.id, t.kind, t.pos, t.reach);

            // Target the living enemy furthest along the path within range.
            let target = self
                .enemies
                .iter()
                .filter(|e| e.hp > 0.0 && e.pos.distance(tpos) <= reach)
                .max_by(|a, b| a.dist.total_cmp(&b.dist))
                .map(|e| (e.id, e.pos, e.dist, e.effective_speed()));
            let Some((eid, epos, edist, espeed)) = target else { continue };

            self.towers[ti].cooldown = st.cooldown;
            self.towers[ti].aim = Some(epos);

            match tkind {
                TowerKind::Deputy => {
                    let id = self.alloc_id();
                    self.projectiles.push(Projectile {
                        id,
                        kind: ProjectileKind::Bullet,
                        tower: tid,
                        pos: tpos,
                        dest: epos,
                        target: Some(eid),
                        speed: st.projectile_speed,
                        damage: st.damage,
                        splash: 0.0,
                        pierce: st.pierce,
                    });
                    self.emit(Event::Shot { tower: tid, kind: tkind, from: tpos, to: epos });
                }
                TowerKind::Gas => {
                    // Lead the target: aim where it will be when the shell lands.
                    let flight = tpos.distance(epos) / st.projectile_speed;
                    let dest = self.map.pos_at(edist + espeed * flight);
                    let id = self.alloc_id();
                    self.projectiles.push(Projectile {
                        id,
                        kind: ProjectileKind::Shell,
                        tower: tid,
                        pos: tpos,
                        dest,
                        target: None,
                        speed: st.projectile_speed,
                        damage: st.damage,
                        splash: st.splash,
                        pierce: st.pierce,
                    });
                    self.emit(Event::Shot { tower: tid, kind: tkind, from: tpos, to: dest });
                }
                TowerKind::Marksman => {
                    if let Some(e) = self.enemies.iter_mut().find(|e| e.id == eid) {
                        hit(e, st.damage, st.pierce, tid);
                    }
                    self.emit(Event::Shot { tower: tid, kind: tkind, from: tpos, to: epos });
                }
                TowerKind::Jammer => {
                    for e in self.enemies.iter_mut().filter(|e| e.hp > 0.0) {
                        if e.pos.distance(tpos) <= reach {
                            hit(e, st.damage, st.pierce, tid);
                            apply_slow(e, st.slow, st.slow_time);
                        }
                    }
                    self.emit(Event::Pulse { tower: tid, pos: tpos, radius: reach });
                }
            }
        }
    }

    /// Gunners shoot the nearest tower in range; everyone near IT's door attacks the server room.
    fn rebels_act(&mut self) {
        let it = it_center();
        let depot = supply_center();
        let (p_gun, p_tools) = supply_odds(self.wave);
        let mut it_damage = 0.0;
        for ei in 0..self.enemies.len() {
            let e = &mut self.enemies[ei];
            if e.hp <= 0.0 {
                continue;
            }
            // Supply hands out guns and tools to rebels passing the depot.
            if !e.supplied && e.pos.distance(depot) <= SUPPLY_REACH {
                e.supplied = true;
                if p_gun > 0.0 || p_tools > 0.0 {
                    let leader = e.kind == EnemyKind::Leader;
                    let gun = leader || self.rng.range_f32(0.0, 1.0) < p_gun;
                    let tools = leader || self.rng.range_f32(0.0, 1.0) < p_tools;
                    let e = &mut self.enemies[ei];
                    e.armed |= gun;
                    e.tools |= tools;
                    if gun || tools {
                        let (id, pos) = (e.id, e.pos);
                        self.totals.armed += gun as u32;
                        self.totals.tooled += tools as u32;
                        self.emit(Event::Equipped { id, pos, gun, tools });
                    }
                }
            }
            let e = &mut self.enemies[ei];
            if self.it_down <= 0.0 && e.pos.distance(it) <= IT_REACH {
                it_damage += (e.kind.it_dps(self.wave) + if e.armed { 6.0 } else { 0.0 }) * TICK;
            }
            if e.blocked {
                let dmg = barrier_dps(e.tools, self.wave) * TICK;
                let edist = e.dist;
                if let Some(b) = self
                    .barriers
                    .iter_mut()
                    .filter(|b| !b.broken && b.dist > edist)
                    .min_by(|a, b| a.dist.total_cmp(&b.dist))
                {
                    b.hp -= dmg;
                }
            }
            let e = &mut self.enemies[ei];
            if !e.armed {
                continue;
            }
            let gun = rebel_gun(self.wave);
            e.gun_cooldown = (e.gun_cooldown - TICK).max(0.0);
            if e.gun_cooldown > 0.0 {
                continue;
            }
            let (eid, epos) = (e.id, e.pos);
            let target = self
                .towers
                .iter()
                .filter(|t| t.pos.distance(epos) <= gun.range)
                .min_by(|a, b| a.pos.distance(epos).total_cmp(&b.pos.distance(epos)).then(a.id.cmp(&b.id)))
                .map(|t| t.id);
            let Some(tid) = target else { continue };
            self.enemies[ei].gun_cooldown = gun.cooldown;
            let ti = self.towers.iter().position(|t| t.id == tid).unwrap();
            let tpos = self.towers[ti].pos;
            self.towers[ti].hp -= gun.damage;
            self.emit(Event::EnemyShot { enemy: eid, tower: tid, from: epos, to: tpos });
            if self.towers[ti].hp <= 0.0 {
                let t = self.towers.remove(ti);
                self.totals.towers_lost += 1;
                // Shells already in the air still land; they just credit nobody.
                self.emit(Event::TowerDestroyed { id: t.id, kind: t.kind, pos: t.pos });
            }
        }
        for i in 0..self.barriers.len() {
            let b = &mut self.barriers[i];
            if !b.broken && b.hp <= 0.0 {
                b.hp = 0.0;
                b.broken = true;
                let (id, pos) = (b.id, b.pos);
                self.totals.barriers_broken += 1;
                self.emit(Event::BarrierBroken { id, pos });
            }
        }
        if self.it_down > 0.0 {
            self.it_down -= TICK;
            if self.it_down <= 0.0 {
                self.it_down = 0.0;
                self.it_hp = IT_HP;
                self.emit(Event::ItRestored);
            }
        } else if it_damage > 0.0 {
            self.it_hp -= it_damage;
            if self.it_hp <= 0.0 {
                self.it_hp = 0.0;
                self.it_down = IT_DOWN_TIME;
                self.emit(Event::ItCollapsed { seconds: IT_DOWN_TIME });
            }
        }
    }

    /// Late in the war Mechanical cuts the main power: one whole section of the silo goes dark
    /// at a time.
    fn update_power(&mut self) {
        for b in &mut self.blackouts {
            b.time_left -= TICK;
        }
        self.blackouts.retain(|b| b.time_left > 0.0);
        if self.wave < POWER_CUT_WAVE || !self.blackouts.is_empty() {
            return;
        }
        self.power_timer -= TICK;
        if self.power_timer > 0.0 {
            return;
        }
        self.power_timer = self.rng.range_f32(8.0, 16.0);
        let (_, first, last) = SECTIONS[self.rng.below(SECTIONS.len())];
        let seconds = self.rng.range_f32(8.0, 12.0);
        self.blackouts.push(Blackout { first, last, time_left: seconds });
        self.emit(Event::PowerCut { first, last, seconds });
    }

    fn move_projectiles(&mut self) {
        let mut i = 0;
        while i < self.projectiles.len() {
            let p = &mut self.projectiles[i];
            if p.kind == ProjectileKind::Bullet {
                match p.target.and_then(|id| self.enemies.iter().find(|e| e.id == id && e.hp > 0.0)) {
                    Some(e) => p.dest = e.pos,
                    None => p.target = None,
                }
            }
            let (pos, arrived) = p.pos.step_toward(p.dest, p.speed * TICK);
            p.pos = pos;
            if !arrived {
                i += 1;
                continue;
            }
            let p = self.projectiles.remove(i);
            match p.kind {
                ProjectileKind::Bullet => {
                    if let Some(e) =
                        p.target.and_then(|id| self.enemies.iter_mut().find(|e| e.id == id && e.hp > 0.0))
                    {
                        hit(e, p.damage, p.pierce, p.tower);
                    }
                }
                ProjectileKind::Shell => {
                    for e in self.enemies.iter_mut().filter(|e| e.hp > 0.0) {
                        if e.pos.distance(p.dest) <= p.splash {
                            hit(e, p.damage, p.pierce, p.tower);
                        }
                    }
                    self.emit(Event::Explosion { pos: p.dest, radius: p.splash });
                }
            }
        }
    }

    fn collect_dead(&mut self) {
        let mut i = 0;
        while i < self.enemies.len() {
            if self.enemies[i].hp > 0.0 {
                i += 1;
                continue;
            }
            let e = self.enemies.remove(i);
            self.chits += e.bounty;
            self.totals.chits_earned += e.bounty;
            self.totals.kills += 1;
            if let Some(t) = e.last_hit_by.and_then(|id| self.towers.iter_mut().find(|t| t.id == id)) {
                t.kills += 1;
            }
            self.emit(Event::EnemyKilled {
                id: e.id,
                kind: e.kind,
                pos: e.pos,
                dist: e.dist,
                bounty: e.bounty,
            });
        }
    }

    fn check_wave_end(&mut self) {
        if !(self.spawn_queue.is_empty() && self.enemies.is_empty()) {
            return;
        }
        self.projectiles.clear();
        // Between waves IT repairs everything and the power comes back.
        self.blackouts.clear();
        for t in &mut self.towers {
            t.hp = t.max_hp;
        }
        let mut rebuilt = 0;
        let mut patched = false;
        for b in &mut self.barriers {
            rebuilt += b.broken as u32;
            patched |= b.hp < b.max_hp;
            b.broken = false;
            b.hp = b.max_hp;
        }
        if patched {
            self.emit(Event::BarriersRepaired { rebuilt });
        }
        if self.it_down > 0.0 || self.it_hp < IT_HP {
            let was_down = self.it_down > 0.0;
            self.it_down = 0.0;
            self.it_hp = IT_HP;
            if was_down {
                self.emit(Event::ItRestored);
            }
        }
        if self.wave >= MAX_WAVES {
            self.phase = Phase::Won;
            self.emit(Event::Won);
        } else {
            self.phase = Phase::Building;
            let bonus = wave_bonus(self.wave);
            self.chits += bonus;
            self.totals.chits_earned += bonus;
            self.emit(Event::WaveCleared { wave: self.wave, bonus });
        }
    }

    // ------------------------------------------------------------ helpers

    fn check_live(&self) -> Result<(), CommandError> {
        if self.is_over() { Err(CommandError::GameOver) } else { Ok(()) }
    }

    fn afford(&self, cost: u32) -> Result<(), CommandError> {
        if self.chits >= cost {
            Ok(())
        } else {
            Err(CommandError::NotEnoughChits { need: cost, have: self.chits })
        }
    }

    fn spend(&mut self, cost: u32) {
        debug_assert!(self.chits >= cost);
        self.chits -= cost;
        self.totals.chits_spent += cost;
    }

    fn alloc_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn emit(&mut self, e: Event) {
        if self.events.len() >= MAX_BUFFERED_EVENTS {
            // Frontend isn't draining; drop the oldest half rather than grow forever.
            self.events.drain(..MAX_BUFFERED_EVENTS / 2);
        }
        self.events.push(e);
    }

    // ------------------------------------------------------------ verification

    /// Stable FNV-1a hash of the full simulation state, for determinism checks and replays.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: u64| {
            for b in v.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        mix(self.tick);
        mix(self.chits as u64);
        mix(self.seals as u64);
        mix(self.wave as u64);
        mix(self.phase as u64);
        mix(self.next_id as u64);
        mix(self.spawn_queue.len() as u64);
        mix(self.spawn_timer.to_bits() as u64);
        for t in &self.towers {
            mix(t.id as u64);
            mix(((t.col as u64) << 32) | t.row as u32 as u64);
            mix(t.level as u64);
            mix(t.cooldown.to_bits() as u64);
            mix(t.hp.to_bits() as u64);
        }
        for e in &self.enemies {
            mix(e.id as u64);
            mix(e.hp.to_bits() as u64);
            mix(e.dist.to_bits() as u64);
            mix(e.slow.to_bits() as u64);
            mix(e.slow_time.to_bits() as u64);
            mix(e.lane.to_bits() as u64);
            mix(e.gun_cooldown.to_bits() as u64);
            mix(e.armed as u64 | (e.tools as u64) << 1 | (e.supplied as u64) << 2 | (e.blocked as u64) << 3);
        }
        for b in &self.barriers {
            mix(b.id as u64);
            mix(b.hp.to_bits() as u64);
            mix(b.level as u64 | (b.broken as u64) << 8);
        }
        mix(self.it_hp.to_bits() as u64);
        mix(self.it_down.to_bits() as u64);
        mix(self.power_timer.to_bits() as u64);
        for b in &self.blackouts {
            mix(((b.first as u64) << 32) | b.last as u32 as u64);
            mix(b.time_left.to_bits() as u64);
        }
        for p in &self.projectiles {
            mix(p.id as u64);
            mix(p.pos.x.to_bits() as u64);
            mix(p.pos.y.to_bits() as u64);
        }
        h
    }

    /// Check internal invariants. Used heavily by tests; cheap enough to call in debug builds.
    pub fn validate(&self) -> Result<(), String> {
        let len = self.map.length();
        if self.seals > START_SEALS {
            return Err(format!("seals {} > start", self.seals));
        }
        if (self.phase == Phase::Lost) != (self.seals == 0) {
            return Err(format!("phase {:?} with seals {}", self.phase, self.seals));
        }
        // On Lost the final scene is kept (enemies frozen at the airlock) for renderers.
        if matches!(self.phase, Phase::Building | Phase::Won)
            && (!self.enemies.is_empty() || !self.spawn_queue.is_empty())
        {
            return Err(format!("{:?} phase with live enemies/spawns", self.phase));
        }
        if self.phase == Phase::Won && self.wave != MAX_WAVES {
            return Err("won before final wave".into());
        }
        let t = &self.totals;
        let expected = START_CHITS as i64 + t.chits_earned as i64 + t.chits_refunded as i64
            - t.chits_spent as i64;
        if expected != self.chits as i64 {
            return Err(format!("chit ledger mismatch: have {} expected {expected}", self.chits));
        }
        for (i, tw) in self.towers.iter().enumerate() {
            if !self.map.tower_allowed(tw.kind, tw.col, tw.row) {
                return Err(format!("tower {} on unbuildable tile", tw.id));
            }
            if self.towers[i + 1..].iter().any(|o| (o.col, o.row) == (tw.col, tw.row)) {
                return Err(format!("two towers at ({},{})", tw.col, tw.row));
            }
            if !(1..=MAX_TOWER_LEVEL).contains(&tw.level) || !tw.cooldown.is_finite() || tw.cooldown < 0.0 {
                return Err(format!("tower {} bad state {:?}", tw.id, tw));
            }
            let range = tw.stats().range;
            if !(tw.reach >= range && tw.reach <= 3.0 * range + 1e-3) {
                return Err(format!("tower {} reach {} vs range {range}", tw.id, tw.reach));
            }
            if !(tw.hp > 0.0 && tw.hp <= tw.max_hp + 1e-3) || tw.max_hp != tw.kind.max_hp(tw.level) {
                return Err(format!("tower {} hp {} / {}", tw.id, tw.hp, tw.max_hp));
            }
        }
        for e in &self.enemies {
            if !(e.hp > 0.0 && e.hp <= e.max_hp) {
                return Err(format!("enemy {} hp {} / {}", e.id, e.hp, e.max_hp));
            }
            if !(0.0..len).contains(&e.dist) || !e.pos.is_finite() {
                return Err(format!("enemy {} at dist {} pos {:?}", e.id, e.dist, e.pos));
            }
            if !(0.0..1.0).contains(&e.slow) || e.slow_time < 0.0 {
                return Err(format!("enemy {} slow {} for {}", e.id, e.slow, e.slow_time));
            }
        }
        for (i, b) in self.barriers.iter().enumerate() {
            let hp_ok = if b.broken { b.hp == 0.0 } else { b.hp > 0.0 && b.hp <= b.max_hp + 1e-3 };
            if !self.map.barrier_allowed(b.col, b.row)
                || !hp_ok
                || !(1..=BARRIER_MAX_LEVEL).contains(&b.level)
                || b.max_hp != barrier_hp(b.level)
            {
                return Err(format!("bad barrier {b:?}"));
            }
            if b.broken && matches!(self.phase, Phase::Building | Phase::Won) {
                return Err(format!("barrier {} still wrecked between waves", b.id));
            }
            if self.barriers[i + 1..].iter().any(|o| (o.col, o.row) == (b.col, b.row)) {
                return Err(format!("two barriers at ({},{})", b.col, b.row));
            }
        }
        if !(0.0..=IT_HP).contains(&self.it_hp) || self.it_down < 0.0 || (self.it_down > 0.0) != (self.it_hp == 0.0) {
            return Err(format!("IT hp {} down {}", self.it_hp, self.it_down));
        }
        if self.phase != Phase::Wave && !self.blackouts.is_empty() && self.phase != Phase::Lost {
            return Err("blackout outside a wave".into());
        }
        for b in &self.blackouts {
            if !(1 <= b.first && b.first <= b.last && b.last < ROWS) || b.time_left <= 0.0 {
                return Err(format!("bad blackout {b:?}"));
            }
        }
        if self.enemies.windows(2).any(|w| w[0].id >= w[1].id) {
            return Err("enemies not sorted by id".into());
        }
        for p in &self.projectiles {
            if !p.pos.is_finite() || !p.dest.is_finite() {
                return Err(format!("projectile {} non-finite", p.id));
            }
        }
        Ok(())
    }
}

/// Apply damage after armor. Non-piercing hits always deal at least 25%.
fn hit(e: &mut Enemy, damage: f32, pierce: bool, tower: Id) {
    if e.hp <= 0.0 {
        return;
    }
    e.hp -= mitigated(damage, e.armor, pierce);
    e.last_hit_by = Some(tower);
}

pub fn mitigated(damage: f32, armor: f32, pierce: bool) -> f32 {
    if pierce { damage } else { (damage - armor).max(damage * 0.25) }
}

/// Slows don't stack: keep the strongest, refresh the longest.
fn apply_slow(e: &mut Enemy, amount: f32, time: f32) {
    e.slow = e.slow.max(amount).min(0.9);
    e.slow_time = e.slow_time.max(time);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_mitigation() {
        assert_eq!(mitigated(12.0, 4.0, false), 8.0);
        assert_eq!(mitigated(12.0, 100.0, false), 3.0);
        assert_eq!(mitigated(12.0, 100.0, true), 12.0);
        assert_eq!(mitigated(12.0, 0.0, false), 12.0);
    }

    #[test]
    fn slows_do_not_stack() {
        let mut g = Game::new(1);
        g.send_wave().unwrap();
        while g.enemies.is_empty() {
            g.step(); // the rally at the generator hall comes first
        }
        let e = &mut g.enemies[0];
        apply_slow(e, 0.4, 1.5);
        apply_slow(e, 0.4, 1.0);
        assert_eq!((e.slow, e.slow_time), (0.4, 1.5));
        apply_slow(e, 0.6, 0.5);
        assert_eq!((e.slow, e.slow_time), (0.6, 1.5));
        apply_slow(e, 5.0, 0.5);
        assert_eq!(e.slow, 0.9);
    }

    #[test]
    fn dead_enemy_ignores_further_hits() {
        let mut g = Game::new(1);
        g.send_wave().unwrap();
        while g.enemies.is_empty() {
            g.step(); // the rally at the generator hall comes first
        }
        let e = &mut g.enemies[0];
        e.hp = 1.0;
        hit(e, 50.0, true, 99);
        let hp = e.hp;
        hit(e, 50.0, true, 100);
        assert_eq!(e.hp, hp);
        assert_eq!(e.last_hit_by, Some(99));
    }
}
