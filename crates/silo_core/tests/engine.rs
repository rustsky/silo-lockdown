use silo_core::bot::{Bot, simulate};
use silo_core::rng::Rng;
use silo_core::stats::*;
use silo_core::*;

const WAVE_CAP: u64 = 30 * 60 * 10;

fn step_checked(g: &mut Game) {
    g.step();
    if let Err(e) = g.validate() {
        panic!("invariant broken at tick {}: {e}", g.tick());
    }
}

// ---------------------------------------------------------------- commands

#[test]
fn initial_state() {
    let g = Game::new(0);
    assert_eq!(g.phase(), Phase::Building);
    assert_eq!(g.chits(), START_CHITS);
    assert_eq!(g.seals(), START_SEALS);
    assert_eq!(g.wave(), 0);
    assert!(g.towers().is_empty() && g.enemies().is_empty());
    g.validate().unwrap();
}

#[test]
fn placement_rules() {
    let mut g = Game::new(0);
    assert_eq!(g.place(TowerKind::Deputy, 7, 11), Err(CommandError::NotBuildable)); // stairs
    assert_eq!(g.place(TowerKind::Deputy, 5, 0), Err(CommandError::NotBuildable)); // airlock
    assert_eq!(g.place(TowerKind::Deputy, 2, 0), Err(CommandError::NotBuildable)); // outside
    assert_eq!(g.place(TowerKind::Deputy, 0, 5), Err(CommandError::NotBuildable)); // rock wall
    assert_eq!(g.place(TowerKind::Deputy, -1, 3), Err(CommandError::OutOfBounds));
    assert_eq!(g.place(TowerKind::Deputy, 3, 13), Err(CommandError::OutOfBounds));

    let id = g.place(TowerKind::Deputy, 4, 10).unwrap();
    assert_eq!(g.chits(), START_CHITS - 50);
    assert_eq!(g.tower_at(4, 10).unwrap().id, id);
    assert_eq!(g.place(TowerKind::Jammer, 4, 10), Err(CommandError::Occupied));

    g.place(TowerKind::Deputy, 5, 10).unwrap();
    // 50 left; Gas costs 100.
    assert_eq!(
        g.place(TowerKind::Gas, 2, 10),
        Err(CommandError::NotEnoughChits { need: 100, have: 50 })
    );
    assert_eq!(g.chits(), 50, "failed command must not charge");
    g.validate().unwrap();
}

#[test]
fn upgrade_and_sell() {
    let mut g = Game::new(0);
    let id = g.place(TowerKind::Deputy, 4, 10).unwrap(); // 100 left
    let c1 = TowerKind::Deputy.upgrade_cost(1).unwrap(); // 40
    assert_eq!(g.upgrade(id), Ok(2));
    assert_eq!(g.chits(), 100 - c1);
    let c2 = TowerKind::Deputy.upgrade_cost(2).unwrap(); // 80
    assert!(g.chits() < c2);
    assert!(matches!(g.upgrade(id), Err(CommandError::NotEnoughChits { .. })));
    assert_eq!(g.tower(id).unwrap().level, 2);

    let invested = 50 + c1;
    assert_eq!(g.tower(id).unwrap().invested, invested);
    let before = g.chits();
    let refund = g.sell(id).unwrap();
    assert_eq!(refund, (invested as f32 * SELL_REFUND).floor() as u32);
    assert_eq!(g.chits(), before + refund);
    assert!(g.tower_at(4, 10).is_none());
    assert_eq!(g.sell(id), Err(CommandError::NoSuchTower));
    assert_eq!(g.upgrade(id), Err(CommandError::NoSuchTower));
    // Tile is free again.
    g.place(TowerKind::Deputy, 4, 10).unwrap();
    g.validate().unwrap();
}

#[test]
fn max_level() {
    let mut g = Game::new(0);
    let id = g.place(TowerKind::Deputy, 4, 10).unwrap();
    // Earn money by surviving waves undefended-ish isn't reliable; use a rich bot run instead:
    // cheaper to just verify via repeated sells/places is impossible, so play waves.
    while g.chits() < 200 {
        g.send_wave().unwrap();
        g.run_wave(WAVE_CAP);
    }
    assert_eq!(g.upgrade(id), Ok(2));
    assert_eq!(g.upgrade(id), Ok(3));
    assert_eq!(g.upgrade(id), Err(CommandError::MaxLevel));
}

#[test]
fn send_wave_rules() {
    let mut g = Game::new(0);
    assert_eq!(g.send_wave(), Ok(1));
    assert_eq!(g.phase(), Phase::Wave);
    assert_eq!(g.send_wave(), Err(CommandError::WaveInProgress));
    // Building during a wave is allowed.
    g.place(TowerKind::Deputy, 4, 10).unwrap();
}

#[test]
fn step_is_noop_while_building() {
    let mut g = Game::new(0);
    let h = g.state_hash();
    for _ in 0..100 {
        g.step();
    }
    assert_eq!(g.state_hash(), h);
}

// ---------------------------------------------------------------- simulation

#[test]
fn undefended_wave_one_breaches_exactly() {
    let mut g = Game::new(0);
    g.send_wave().unwrap();
    let porters = wave(1).len() as u32;
    let mut breaches = 0;
    while g.phase() == Phase::Wave {
        step_checked(&mut g);
        for e in g.drain_events() {
            if let Event::Breach { seals_lost, .. } = e {
                breaches += seals_lost;
            }
        }
    }
    assert_eq!(breaches, porters);
    assert_eq!(g.seals(), START_SEALS - porters);
    assert_eq!(g.phase(), Phase::Building);
    assert_eq!(g.chits(), START_CHITS + wave_bonus(1));
    // Crossing time sanity: path ~34 tiles at 0.95 t/s ~= 36s for the fastest rebels.
    assert!(g.time() > 30.0 && g.time() < 60.0, "wave took {}s", g.time());
}

#[test]
fn undefended_silo_falls() {
    let mut g = Game::new(0);
    let mut lost_events = 0;
    while !g.is_over() {
        g.send_wave().unwrap();
        while g.phase() == Phase::Wave {
            step_checked(&mut g);
        }
        lost_events += g.drain_events().iter().filter(|e| **e == Event::Lost).count();
    }
    assert_eq!(g.phase(), Phase::Lost);
    assert_eq!(g.seals(), 0);
    assert_eq!(lost_events, 1);
    assert!(g.wave() <= 3, "undefended silo survived to wave {}", g.wave());
    // Game over: every command refused, step does nothing.
    assert_eq!(g.send_wave(), Err(CommandError::GameOver));
    assert_eq!(g.place(TowerKind::Deputy, 4, 10), Err(CommandError::GameOver));
    let h = g.state_hash();
    g.step();
    assert_eq!(g.state_hash(), h);
}

#[test]
fn deputies_kill_porters_and_earn_bounty() {
    let mut g = Game::new(0);
    // Cover the first landing from both sides.
    g.place(TowerKind::Deputy, 4, 11).unwrap();
    g.place(TowerKind::Deputy, 5, 10).unwrap();
    g.send_wave().unwrap();
    g.run_wave(WAVE_CAP);
    let t = g.totals();
    assert_eq!(t.kills + t.breaches, wave(1).len() as u32);
    assert!(t.kills >= 4, "only {} kills", t.kills);
    let kills: u32 = g.towers().iter().map(|t| t.kills).sum();
    assert_eq!(kills, t.kills, "kill credit must add up");
    g.validate().unwrap();
}

#[test]
fn jammer_slows_enemies() {
    let travel = |with_jammer: bool| -> f32 {
        let mut g = Game::new(0);
        if with_jammer {
            g.place(TowerKind::Jammer, 2, 12).unwrap();
        }
        g.send_wave().unwrap();
        let mut slowed = false;
        for _ in 0..(30 * 4) {
            g.step();
            slowed |= g.enemies().iter().any(|e| e.is_slowed());
        }
        assert_eq!(slowed, with_jammer);
        g.enemies().iter().find(|e| e.id == g.enemies()[0].id).unwrap().dist
    };
    let free = travel(false);
    let jammed = travel(true);
    assert!(jammed < free * 0.85, "jammed {jammed} vs free {free}");
}

#[test]
fn gas_splash_hits_multiple() {
    let mut g = Game::new(0);
    g.place(TowerKind::Gas, 4, 10).unwrap();
    g.send_wave().unwrap();
    let mut max_hit_by_one_blast = 0;
    while g.phase() == Phase::Wave {
        let before: Vec<_> = g.enemies().iter().map(|e| (e.id, e.hp)).collect();
        g.step();
        let events = g.drain_events();
        if events.iter().any(|e| matches!(e, Event::Explosion { .. })) {
            let damaged = before
                .iter()
                .filter(|(id, hp)| g.enemies().iter().find(|e| e.id == *id).is_none_or(|e| e.hp < *hp))
                .count();
            max_hit_by_one_blast = max_hit_by_one_blast.max(damaged);
        }
    }
    assert!(max_hit_by_one_blast >= 2, "splash never hit 2+ ({max_hit_by_one_blast})");
}

#[test]
fn marksman_pierces_armor() {
    // Wave 4 brings armored miners. Survive to it with deputies, then add a marksman.
    let mut g = Game::new(0);
    g.place(TowerKind::Deputy, 4, 11).unwrap();
    g.place(TowerKind::Deputy, 5, 10).unwrap();
    for _ in 0..3 {
        g.send_wave().unwrap();
        g.run_wave(WAVE_CAP);
    }
    assert_eq!(g.phase(), Phase::Building, "two deputies should survive 3 waves");
    let id = g.place(TowerKind::Marksman, 9, 4).unwrap();
    g.send_wave().unwrap();
    let dmg = TowerKind::Marksman.stats(1).damage;
    let mut verified = false;
    while g.phase() == Phase::Wave && !verified {
        let before: Vec<_> = g.enemies().iter().map(|e| (e.id, e.kind, e.hp)).collect();
        g.step();
        let marksman_fired = g
            .drain_events()
            .iter()
            .any(|ev| matches!(ev, Event::Shot { tower, .. } if *tower == id));
        if !marksman_fired {
            continue;
        }
        for (eid, kind, hp) in &before {
            let Some(e) = g.enemies().iter().find(|e| e.id == *eid) else { continue };
            // Only the marksman shot can deal exactly its full damage to an armored miner.
            if *kind == EnemyKind::Miner && (hp - e.hp - dmg).abs() < 1e-3 {
                verified = true;
            }
        }
    }
    assert!(verified, "never saw a full-damage marksman hit on a miner");
}

#[test]
fn selling_tower_mid_flight_is_safe() {
    let mut g = Game::new(3);
    let id = g.place(TowerKind::Deputy, 2, 10).unwrap();
    g.send_wave().unwrap();
    while g.projectiles().is_empty() && g.phase() == Phase::Wave {
        g.step();
    }
    g.sell(id).unwrap();
    while g.phase() == Phase::Wave {
        step_checked(&mut g);
    }
}

// ---------------------------------------------------------------- determinism

fn scripted(seed: u64) -> Vec<u64> {
    let mut g = Game::new(seed);
    let mut bot = Bot::balanced(g.map());
    let mut hashes = Vec::new();
    while !g.is_over() && g.wave() < 8 {
        bot.take_turn(&mut g);
        while g.phase() == Phase::Wave {
            g.step();
            hashes.push(g.state_hash());
        }
    }
    hashes
}

#[test]
fn same_seed_same_game() {
    assert_eq!(scripted(42), scripted(42));
    assert_ne!(scripted(1), scripted(2), "lanes are seeded, hashes should differ");
}

#[test]
fn clone_is_independent_snapshot() {
    let mut g = Game::new(5);
    g.place(TowerKind::Deputy, 4, 11).unwrap();
    g.send_wave().unwrap();
    for _ in 0..200 {
        g.step();
    }
    let mut snap = g.clone();
    for _ in 0..300 {
        g.step();
        snap.step();
    }
    assert_eq!(g.state_hash(), snap.state_hash());
}

// ---------------------------------------------------------------- fuzz

/// Random players issuing random (often invalid) commands at random times. Nothing may panic
/// and every invariant (incl. the chit ledger) must hold after every tick.
#[test]
fn fuzz_random_commands() {
    for seed in 0..60u64 {
        let mut g = Game::new(seed);
        let mut r = Rng::new(seed ^ 0xDEAD_BEEF);
        let mut ticks = 0u64;
        while !g.is_over() && ticks < 30 * 60 * 40 {
            if r.below(20) == 0 || g.phase() == Phase::Building {
                let cmd = match r.below(6) {
                    0 | 1 => Command::Place {
                        kind: TowerKind::ALL[r.below(4)],
                        col: r.below(12) as i32 - 1,
                        row: r.below(14) as i32 - 1,
                    },
                    2 => Command::Upgrade { tower: r.below(g.towers().len() + 3) as u32 },
                    3 if r.below(4) == 0 => Command::Sell {
                        tower: g.towers().get(r.below(g.towers().len().max(1))).map_or(0, |t| t.id),
                    },
                    _ => Command::SendWave,
                };
                let before = g.chits();
                if g.apply(cmd).is_err() {
                    assert_eq!(g.chits(), before, "failed {cmd:?} changed chits");
                }
            }
            step_checked(&mut g);
            ticks += 1;
            if ticks.is_multiple_of(600) {
                g.drain_events();
            }
        }
    }
}

// ---------------------------------------------------------------- balance

#[test]
fn balanced_bot_wins_most_seeds() {
    let seeds = 0..12u64;
    let n = seeds.clone().count();
    let mut wins = 0;
    let map = Map::new();
    for seed in seeds {
        let r = simulate(seed, &mut Bot::balanced(&map));
        if r.phase == Phase::Won {
            wins += 1;
            assert!(r.seals >= 5, "won but barely ({r:?}): too hard");
        }
    }
    assert!(wins * 4 >= n * 3, "balanced bot won only {wins}/{n}");
}

#[test]
fn single_deputy_spam_struggles() {
    // A lazy strategy should not trivially win: game must have some challenge.
    let g = Game::new(0);
    let mut bot = Bot::with_plan(g.map(), vec![TowerKind::Deputy]);
    let r = simulate(0, &mut bot);
    assert!(
        r.phase == Phase::Lost || r.seals < START_SEALS,
        "deputy spam won flawlessly: {r:?}"
    );
}

#[test]
fn mono_strategies_lose() {
    // Gas-only and marksman-only must not beat the game: mixing towers should matter.
    let map = Map::new();
    for kind in [TowerKind::Gas, TowerKind::Marksman] {
        let r = simulate(0, &mut Bot::with_plan(&map, vec![kind]));
        assert_eq!(r.phase, Phase::Lost, "{kind:?}-only won: {r:?}");
    }
}

/// Bot keeps the silo alive so late waves (bosses, armor, big crowds) get fuzzed too, while
/// random mid-wave sells/upgrades/placements stir things up.
#[test]
fn fuzz_late_game_chaos() {
    let map = Map::new();
    let mut max_wave = 0;
    for seed in 0..16u64 {
        let mut g = Game::new(seed);
        let mut bot = Bot::balanced(&map);
        let mut r = Rng::new(seed.wrapping_mul(31) + 7);
        while !g.is_over() {
            bot.take_turn(&mut g);
            let mut ticks = 0;
            while g.phase() == Phase::Wave {
                assert!(ticks < WAVE_CAP, "wave {} stuck", g.wave());
                if r.below(60) == 0 && !g.towers().is_empty() {
                    let t = g.towers()[r.below(g.towers().len())].id;
                    let cmd = match r.below(25) {
                        0 if g.towers().len() > 8 => Command::Sell { tower: t },
                        _ => Command::Upgrade { tower: t },
                    };
                    let _ = g.apply(cmd);
                    bot.spend(&mut g);
                }
                step_checked(&mut g);
                ticks += 1;
            }
            g.drain_events();
        }
        max_wave = max_wave.max(g.wave());
    }
    assert!(max_wave >= 12, "chaos fuzz never reached late game (max wave {max_wave})");
}
