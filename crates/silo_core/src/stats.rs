//! Balance data: towers (IT security), enemies (the uprising) and waves.

pub const START_CHITS: u32 = 150;
pub const START_SEALS: u32 = 20;
pub const MAX_WAVES: u32 = 20;
pub const MAX_TOWER_LEVEL: u8 = 3;
pub const SELL_REFUND: f32 = 0.6;

// ---------------------------------------------------------------- towers

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TowerKind {
    /// IT security deputy: steady single-target fire.
    Deputy,
    /// Radio jammer: periodic pulse that slows and lightly damages everything in range.
    Jammer,
    /// Gas canister launcher: lobbed shell with splash damage.
    Gas,
    /// Overseer's marksman: long range, slow, ignores armor.
    Marksman,
}

impl TowerKind {
    pub const ALL: [TowerKind; 4] = [Self::Deputy, Self::Jammer, Self::Gas, Self::Marksman];

    pub fn name(self) -> &'static str {
        match self {
            Self::Deputy => "Deputy",
            Self::Jammer => "Jammer",
            Self::Gas => "Gas Launcher",
            Self::Marksman => "Marksman",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Deputy => "IT security. Reliable single-target fire.",
            Self::Jammer => "Scrambles radios. Slows all rebels in range.",
            Self::Gas => "Lobs gas canisters. Splash damage.",
            Self::Marksman => "Long range. Pierces armor.",
        }
    }

    pub fn cost(self) -> u32 {
        match self {
            Self::Deputy => 50,
            Self::Jammer => 70,
            Self::Gas => 100,
            Self::Marksman => 150,
        }
    }

    /// Cost to go from `level` to `level + 1`, or None at max level.
    pub fn upgrade_cost(self, level: u8) -> Option<u32> {
        (level < MAX_TOWER_LEVEL).then(|| (self.cost() as f32 * 0.8 * level as f32).round() as u32)
    }

    pub fn stats(self, level: u8) -> TowerStats {
        let l = (level.clamp(1, MAX_TOWER_LEVEL) - 1) as f32;
        let base = match self {
            Self::Deputy => TowerStats {
                range: 1.9,
                damage: 12.0,
                cooldown: 0.6,
                projectile_speed: 9.0,
                splash: 0.0,
                slow: 0.0,
                slow_time: 0.0,
                pierce: false,
            },
            Self::Jammer => TowerStats {
                range: 1.5,
                damage: 3.0,
                cooldown: 1.0,
                projectile_speed: 0.0,
                splash: 0.0,
                slow: 0.40,
                slow_time: 1.5,
                pierce: true,
            },
            Self::Gas => TowerStats {
                range: 1.7,
                damage: 22.0,
                cooldown: 1.4,
                projectile_speed: 5.0,
                splash: 0.85,
                slow: 0.0,
                slow_time: 0.0,
                pierce: false,
            },
            Self::Marksman => TowerStats {
                range: 2.6,
                damage: 48.0,
                cooldown: 2.3,
                projectile_speed: 0.0,
                splash: 0.0,
                slow: 0.0,
                slow_time: 0.0,
                pierce: true,
            },
        };
        TowerStats {
            range: base.range * (1.0 + 0.12 * l),
            damage: base.damage * (1.0 + 0.6 * l),
            cooldown: base.cooldown * (1.0 - 0.12 * l),
            slow: if base.slow > 0.0 { base.slow + 0.1 * l } else { 0.0 },
            ..base
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TowerStats {
    /// Tiles.
    pub range: f32,
    pub damage: f32,
    /// Seconds between shots.
    pub cooldown: f32,
    /// Tiles/second. 0 = instant (hitscan or pulse).
    pub projectile_speed: f32,
    /// Splash radius in tiles; 0 = single target.
    pub splash: f32,
    /// Speed reduction fraction applied on hit (0..1).
    pub slow: f32,
    pub slow_time: f32,
    /// Ignores armor.
    pub pierce: bool,
}

// ---------------------------------------------------------------- enemies

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnemyKind {
    /// Fast, fragile message runners.
    Porter,
    /// Mechanical workers. The backbone of the uprising.
    Mechanic,
    /// Down Deep miners. Slow and armored.
    Miner,
    /// Rebel leader. Boss.
    Leader,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyStats {
    pub hp: f32,
    /// Tiles/second.
    pub speed: f32,
    /// Flat damage reduction per hit (non-piercing).
    pub armor: f32,
    pub bounty: u32,
    /// Airlock seals destroyed on breach.
    pub breach: u32,
}

impl EnemyKind {
    pub const ALL: [EnemyKind; 4] = [Self::Porter, Self::Mechanic, Self::Miner, Self::Leader];

    pub fn name(self) -> &'static str {
        match self {
            Self::Porter => "Porter",
            Self::Mechanic => "Mechanic",
            Self::Miner => "Miner",
            Self::Leader => "Rebel Leader",
        }
    }

    pub fn base(self) -> EnemyStats {
        match self {
            Self::Porter => EnemyStats { hp: 30.0, speed: 0.95, armor: 0.0, bounty: 4, breach: 1 },
            Self::Mechanic => EnemyStats { hp: 75.0, speed: 0.6, armor: 1.0, bounty: 7, breach: 1 },
            Self::Miner => EnemyStats { hp: 220.0, speed: 0.36, armor: 4.0, bounty: 14, breach: 2 },
            Self::Leader => EnemyStats { hp: 1100.0, speed: 0.28, armor: 5.0, bounty: 90, breach: 5 },
        }
    }

    /// Stats scaled for wave `n` (1-based).
    pub fn for_wave(self, n: u32) -> EnemyStats {
        let k = n.saturating_sub(1) as f32;
        let b = self.base();
        EnemyStats {
            hp: b.hp * hp_multiplier(n),
            armor: b.armor + (k / 3.0).floor() * if b.armor > 0.0 { 1.0 } else { 0.0 },
            bounty: (b.bounty as f32 * (1.0 + 0.02 * k)).round() as u32,
            ..b
        }
    }
}

pub fn hp_multiplier(n: u32) -> f32 {
    let k = n.saturating_sub(1) as f32;
    1.0 + 0.22 * k + 0.026 * k * k
}

/// Chits awarded for clearing wave `n`.
pub fn wave_bonus(n: u32) -> u32 {
    15 + 3 * n
}

// ---------------------------------------------------------------- waves

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn {
    /// Seconds to wait after the previous spawn.
    pub delay: f32,
    pub kind: EnemyKind,
}

/// Build the spawn list for wave `n` (1-based).
pub fn wave(n: u32) -> Vec<Spawn> {
    let mut out = Vec::new();
    let mut group = |kind: EnemyKind, count: u32, gap: f32, lead: f32| {
        for i in 0..count {
            out.push(Spawn { delay: if i == 0 { lead } else { gap }, kind });
        }
    };
    let porters = 4 + n;
    let mechanics = if n >= 2 { 1 + (n as f32 * 0.8) as u32 } else { 0 };
    let miners = if n >= 4 { (n - 2) / 2 } else { 0 };
    let leaders = match n {
        10 | 15 => 1,
        20 => 2,
        _ => 0,
    };

    group(EnemyKind::Porter, porters.div_ceil(2), 0.5, 0.0);
    group(EnemyKind::Mechanic, mechanics, 0.8, 1.5);
    group(EnemyKind::Porter, porters / 2, 0.45, 1.5);
    group(EnemyKind::Miner, miners, 1.4, 2.0);
    group(EnemyKind::Leader, leaders, 4.0, 3.0);
    out
}

pub fn wave_lore(n: u32) -> &'static str {
    const LORE: [&str; MAX_WAVES as usize] = [
        "Unrest in the Down Deep. Porters carry the word upward.",
        "Mechanical stops answering the radio.",
        "Someone spoke of the outside. The stairwell fills.",
        "Miners leave the Deep with tools in hand.",
        "Supply has opened its doors to them.",
        "The Mids are choosing sides.",
        "Graffiti on the landings: 'WE WANT TO SEE'.",
        "Power flickers. Mechanical is cutting the lines.",
        "IT orders a full lockdown. They climb anyway.",
        "A leader rises from Mechanical.",
        "Rumours of other silos spread on every level.",
        "The Pact is being read aloud in the stairwell.",
        "Deputies are defecting. Hold the landings.",
        "Smoke rises from the Down Deep.",
        "The leader returns, angrier.",
        "They have breached Supply's armory.",
        "Every porter in the silo is running.",
        "The Mids have fallen. Only Up Top remains.",
        "The servers hum. They know what IT hides.",
        "Final push. They want to see the outside.",
    ];
    LORE[(n.clamp(1, MAX_WAVES) - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_strictly_improve_and_cap() {
        for k in TowerKind::ALL {
            let mut prev = k.stats(1);
            for lvl in 2..=MAX_TOWER_LEVEL {
                let s = k.stats(lvl);
                assert!(s.damage > prev.damage);
                assert!(s.range > prev.range);
                assert!(s.cooldown < prev.cooldown && s.cooldown > 0.0);
                assert!(s.slow < 1.0);
                prev = s;
            }
            assert!(k.upgrade_cost(1).unwrap() > 0);
            assert!(k.upgrade_cost(MAX_TOWER_LEVEL).is_none());
            assert_eq!(k.stats(MAX_TOWER_LEVEL + 5), k.stats(MAX_TOWER_LEVEL));
        }
    }

    #[test]
    fn waves_grow_and_bosses_appear() {
        for n in 1..=MAX_WAVES {
            let w = wave(n);
            assert!(!w.is_empty());
            assert!(w.iter().all(|s| s.delay >= 0.0 && s.delay.is_finite()));
            assert!(!wave_lore(n).is_empty());
        }
        // Boss waves spike, so compare the rank-and-file only.
        let total_hp = |n: u32| -> f32 {
            wave(n)
                .iter()
                .filter(|s| s.kind != EnemyKind::Leader)
                .map(|s| s.kind.for_wave(n).hp)
                .sum()
        };
        for n in 2..=MAX_WAVES {
            assert!(total_hp(n) > total_hp(n - 1) * 0.95, "wave {n} weaker than previous");
        }
        let has_leader = |n| wave(n).iter().any(|s| s.kind == EnemyKind::Leader);
        assert!(has_leader(10) && has_leader(15) && has_leader(20));
        assert!(!has_leader(9));
    }
}
