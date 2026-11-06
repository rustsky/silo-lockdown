//! Deterministic fixed-tick simulation. Frontends call `step()` at `TICK` intervals (or several
//! times per frame for fast-forward), issue commands, and render from the read-only accessors and
//! the event stream.

use crate::map::{Map, in_bounds, tile_center};
use crate::math::Vec2;
use crate::rng::Rng;
use crate::stats::*;
use std::collections::VecDeque;

/// Simulation step in seconds (30 Hz).
pub const TICK: f32 = 1.0 / 30.0;
const MAX_BUFFERED_EVENTS: usize = 4096;

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
    last_hit_by: Option<Id>,
}

impl Enemy {
    pub fn effective_speed(&self) -> f32 {
        self.speed * (1.0 - self.slow)
    }
    pub fn is_slowed(&self) -> bool {
        self.slow_time > 0.0 && self.slow > 0.0
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
        }
    }
}

impl std::error::Error for CommandError {}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    WaveStarted { wave: u32, lore: &'static str },
    WaveCleared { wave: u32, bonus: u32 },
    EnemySpawned { id: Id, kind: EnemyKind },
    EnemyKilled { id: Id, kind: EnemyKind, pos: Vec2, bounty: u32 },
    Breach { id: Id, kind: EnemyKind, seals_lost: u32 },
    TowerPlaced { id: Id, kind: TowerKind, col: i32, row: i32 },
    TowerUpgraded { id: Id, level: u8 },
    TowerSold { id: Id, refund: u32 },
    /// A tower fired (bullet/shell launch, or marksman hitscan from->to).
    Shot { tower: Id, kind: TowerKind, from: Vec2, to: Vec2 },
    Pulse { tower: Id, pos: Vec2, radius: f32 },
    Explosion { pos: Vec2, radius: f32 },
    Won,
    Lost,
}

/// Running totals, mostly for end screens and for accounting tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub kills: u32,
    pub breaches: u32,
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
    enemies: Vec<Enemy>,
    projectiles: Vec<Projectile>,
    spawn_queue: VecDeque<Spawn>,
    spawn_timer: f32,
    events: Vec<Event>,
    totals: Totals,
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
            enemies: Vec::new(),
            projectiles: Vec::new(),
            spawn_queue: VecDeque::new(),
            spawn_timer: 0.0,
            events: Vec::new(),
            totals: Totals::default(),
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
    pub fn enemies(&self) -> &[Enemy] {
        &self.enemies
    }
    pub fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }
    pub fn totals(&self) -> &Totals {
        &self.totals
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
        if !self.map.buildable(col, row) {
            return Err(CommandError::NotBuildable);
        }
        if self.tower_at(col, row).is_some() {
            return Err(CommandError::Occupied);
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
            pos: tile_center(col, row),
            level: 1,
            invested: cost,
            cooldown: 0.0,
            aim: None,
            kills: 0,
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

    /// Start the next wave. Returns its number.
    pub fn send_wave(&mut self) -> Result<u32, CommandError> {
        self.check_live()?;
        if self.phase == Phase::Wave {
            return Err(CommandError::WaveInProgress);
        }
        self.wave += 1;
        self.phase = Phase::Wave;
        self.spawn_queue = wave(self.wave).into();
        self.spawn_timer = self.spawn_queue.front().map_or(0.0, |s| s.delay);
        self.emit(Event::WaveStarted { wave: self.wave, lore: wave_lore(self.wave) });
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
        self.move_projectiles();
        self.collect_dead();
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
            e.dist = (e.dist + e.effective_speed() * TICK).min(len);
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
            let (tid, tkind, tpos) = (t.id, t.kind, t.pos);

            // Target the living enemy furthest along the path within range.
            let target = self
                .enemies
                .iter()
                .filter(|e| e.hp > 0.0 && e.pos.distance(tpos) <= st.range)
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
                        if e.pos.distance(tpos) <= st.range {
                            hit(e, st.damage, st.pierce, tid);
                            apply_slow(e, st.slow, st.slow_time);
                        }
                    }
                    self.emit(Event::Pulse { tower: tid, pos: tpos, radius: st.range });
                }
            }
        }
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
            self.emit(Event::EnemyKilled { id: e.id, kind: e.kind, pos: e.pos, bounty: e.bounty });
        }
    }

    fn check_wave_end(&mut self) {
        if !(self.spawn_queue.is_empty() && self.enemies.is_empty()) {
            return;
        }
        self.projectiles.clear();
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
        }
        for e in &self.enemies {
            mix(e.id as u64);
            mix(e.hp.to_bits() as u64);
            mix(e.dist.to_bits() as u64);
            mix(e.slow.to_bits() as u64);
            mix(e.slow_time.to_bits() as u64);
            mix(e.lane.to_bits() as u64);
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
            if !self.map.buildable(tw.col, tw.row) {
                return Err(format!("tower {} on unbuildable tile", tw.id));
            }
            if self.towers[i + 1..].iter().any(|o| (o.col, o.row) == (tw.col, tw.row)) {
                return Err(format!("two towers at ({},{})", tw.col, tw.row));
            }
            if !(1..=MAX_TOWER_LEVEL).contains(&tw.level) || !tw.cooldown.is_finite() || tw.cooldown < 0.0 {
                return Err(format!("tower {} bad state {:?}", tw.id, tw));
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
        g.step();
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
        g.step();
        let e = &mut g.enemies[0];
        e.hp = 1.0;
        hit(e, 50.0, true, 99);
        let hp = e.hp;
        hit(e, 50.0, true, 100);
        assert_eq!(e.hp, hp);
        assert_eq!(e.last_hit_by, Some(99));
    }
}
