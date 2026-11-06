//! Simple greedy AI that plays the IT side. Used for balance simulations, tests,
//! and as an "autoplay" demo in frontends.

use crate::game::{CommandError, Game, Phase};
use crate::map::{COLS, Map, ROWS, tile_center};
use crate::stats::TowerKind;
use std::collections::HashMap;

pub struct Bot {
    plan: Vec<TowerKind>,
    next: usize,
    /// Per-kind list of (col, row, coverage) sorted best-first.
    spots: HashMap<TowerKind, Vec<(i32, i32, usize)>>,
}

impl Bot {
    /// A balanced build order.
    pub fn balanced(map: &Map) -> Self {
        use TowerKind::*;
        Self::with_plan(map, vec![Deputy, Deputy, Jammer, Gas, Deputy, Marksman, Gas, Jammer, Marksman, Deputy])
    }

    pub fn with_plan(map: &Map, plan: Vec<TowerKind>) -> Self {
        assert!(!plan.is_empty());
        let samples: Vec<_> = map.samples(0.25).collect();
        let mut spots = HashMap::new();
        for kind in TowerKind::ALL {
            let range = kind.stats(1).range;
            let mut v: Vec<_> = (0..COLS)
                .flat_map(|c| (0..ROWS).map(move |r| (c, r)))
                .filter(|&(c, r)| map.buildable(c, r))
                .map(|(c, r)| {
                    let p = tile_center(c, r);
                    (c, r, samples.iter().filter(|s| s.distance(p) <= range).count())
                })
                .collect();
            v.sort_by(|a, b| b.2.cmp(&a.2).then((a.0, a.1).cmp(&(b.0, b.1))));
            spots.insert(kind, v);
        }
        Self { plan, next: 0, spots }
    }

    /// Spend chits sensibly. Safe to call any time; does nothing once the game is over.
    pub fn spend(&mut self, g: &mut Game) {
        for _ in 0..64 {
            if g.is_over() {
                return;
            }
            let prefer_place = g.towers().len() < 6 + g.wave() as usize;
            // Order matters: `||` short-circuits, so the first branch gets first pick of chits.
            #[allow(clippy::if_same_then_else)]
            let acted = if prefer_place {
                self.try_place(g) || self.try_upgrade(g)
            } else {
                self.try_upgrade(g) || self.try_place(g)
            };
            if !acted {
                return;
            }
        }
    }

    /// Spend, then start the next wave if between waves.
    pub fn take_turn(&mut self, g: &mut Game) {
        self.spend(g);
        if g.phase() == Phase::Building {
            g.send_wave().expect("send_wave in Building phase must succeed");
        }
    }

    fn try_place(&mut self, g: &mut Game) -> bool {
        let kind = self.plan[self.next % self.plan.len()];
        let Some(&(c, r, _)) = self.spots[&kind].iter().find(|&&(c, r, _)| g.tower_at(c, r).is_none()) else {
            return false;
        };
        match g.place(kind, c, r) {
            Ok(_) => {
                self.next += 1;
                true
            }
            Err(CommandError::NotEnoughChits { .. }) => false,
            Err(e) => panic!("bot placement failed unexpectedly: {e:?}"),
        }
    }

    fn try_upgrade(&mut self, g: &mut Game) -> bool {
        let best = g
            .towers()
            .iter()
            .filter_map(|t| t.upgrade_cost().map(|c| (c, t.id)))
            .filter(|&(c, _)| c <= g.chits())
            .min();
        match best {
            Some((_, id)) => g.upgrade(id).is_ok(),
            None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimResult {
    pub phase: Phase,
    pub wave: u32,
    pub seals: u32,
    pub towers: usize,
    pub kills: u32,
}

/// Play a whole game with `bot`. Each wave is capped so a stuck sim can't hang.
pub fn simulate(seed: u64, bot: &mut Bot) -> SimResult {
    let mut g = Game::new(seed);
    while !g.is_over() {
        bot.take_turn(&mut g);
        let ran = g.run_wave(30 * 60 * 10);
        assert!(g.phase() != crate::game::Phase::Wave, "wave {} did not finish in {ran} ticks", g.wave());
        g.drain_events();
    }
    SimResult {
        phase: g.phase(),
        wave: g.wave(),
        seals: g.seals(),
        towers: g.towers().len(),
        kills: g.totals().kills,
    }
}
