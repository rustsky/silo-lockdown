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
    assert_eq!(g.place(TowerKind::Deputy, 7, 35), Err(CommandError::NotBuildable)); // stairs
    assert_eq!(g.place(TowerKind::Deputy, 5, 0), Err(CommandError::NotBuildable)); // airlock
    assert_eq!(g.place(TowerKind::Deputy, 2, 0), Err(CommandError::NotBuildable)); // outside
    assert_eq!(g.place(TowerKind::Deputy, 0, 5), Err(CommandError::NotBuildable)); // rock wall
    assert_eq!(g.place(TowerKind::Deputy, -1, 3), Err(CommandError::OutOfBounds));
    assert_eq!(g.place(TowerKind::Deputy, 3, 37), Err(CommandError::OutOfBounds));

    let id = g.place(TowerKind::Deputy, 4, 34).unwrap();
    assert_eq!(g.chits(), START_CHITS - 50);
    assert_eq!(g.tower_at(4, 34).unwrap().id, id);
    assert_eq!(g.place(TowerKind::Jammer, 4, 34), Err(CommandError::Occupied));

    g.place(TowerKind::Deputy, 5, 34).unwrap();
    // 50 left; Gas costs 100.
    assert_eq!(
        g.place(TowerKind::Gas, 2, 34),
        Err(CommandError::NotEnoughChits { need: 100, have: 50 })
    );
    assert_eq!(g.chits(), 50, "failed command must not charge");
    g.validate().unwrap();
}

#[test]
fn upgrade_and_sell() {
    let mut g = Game::new(0);
    let id = g.place(TowerKind::Deputy, 4, 34).unwrap(); // 100 left
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
    assert!(g.tower_at(4, 34).is_none());
    assert_eq!(g.sell(id), Err(CommandError::NoSuchTower));
    assert_eq!(g.upgrade(id), Err(CommandError::NoSuchTower));
    // Tile is free again.
    g.place(TowerKind::Deputy, 4, 34).unwrap();
    g.validate().unwrap();
}

#[test]
fn max_level() {
    let mut g = Game::new(0);
    let id = g.place(TowerKind::Deputy, 4, 34).unwrap();
    // Two helpers so the silo survives long enough to save up.
    g.place(TowerKind::Deputy, 4, 35).unwrap();
    g.place(TowerKind::Deputy, 5, 34).unwrap();
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
    g.place(TowerKind::Deputy, 4, 34).unwrap();
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
    // Crossing time sanity: path ~105 tiles at 1.14 t/s ~= 92s for the fastest rebels.
    assert!(g.time() > 75.0 && g.time() < 130.0, "wave took {}s", g.time());
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
    assert_eq!(g.place(TowerKind::Deputy, 4, 34), Err(CommandError::GameOver));
    let h = g.state_hash();
    g.step();
    assert_eq!(g.state_hash(), h);
}

#[test]
fn deputies_kill_porters_and_earn_bounty() {
    let mut g = Game::new(0);
    // Cover the first landing from both sides.
    g.place(TowerKind::Deputy, 4, 35).unwrap();
    g.place(TowerKind::Deputy, 5, 34).unwrap();
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
            // The buildable tile closest to where the stair starts.
            let start = g.map().pos_at(0.0);
            let (c, r) = (0..map::COLS)
                .flat_map(|c| (0..map::ROWS).map(move |r| (c, r)))
                .filter(|&(c, r)| g.map().buildable(c, r))
                .min_by(|a, b| {
                    let d = |&(c, r): &(i32, i32)| map::tile_center(c, r).distance(start);
                    d(a).total_cmp(&d(b))
                })
                .unwrap();
            g.place(TowerKind::Jammer, c, r).unwrap();
        }
        g.send_wave().unwrap();
        let mut slowed = false;
        for _ in 0..((RALLY_TIME + 4.0) * 30.0) as u32 {
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
    g.place(TowerKind::Gas, 4, 34).unwrap();
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
    g.place(TowerKind::Deputy, 4, 35).unwrap();
    g.place(TowerKind::Deputy, 5, 34).unwrap();
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
    let id = g.place(TowerKind::Deputy, 2, 34).unwrap();
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
    g.place(TowerKind::Deputy, 4, 35).unwrap();
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
                let cmd = match r.below(7) {
                    0 | 1 => Command::Place {
                        kind: TowerKind::ALL[r.below(4)],
                        col: r.below(12) as i32 - 1,
                        row: r.below(map::ROWS as usize + 2) as i32 - 1,
                    },
                    4 if r.below(3) == 0 => Command::PlaceBarrier {
                        col: r.below(12) as i32 - 1,
                        row: r.below(map::ROWS as usize + 2) as i32 - 1,
                    },
                    4 => Command::RemoveBarrier {
                        barrier: g.barriers().get(r.below(g.barriers().len().max(1))).map_or(0, |b| b.id),
                    },
                    2 => Command::Upgrade { tower: r.below(g.towers().len() + 3) as u32 },
                    5 if r.below(2) == 0 => Command::UpgradeBarrier {
                        barrier: g.barriers().get(r.below(g.barriers().len().max(1))).map_or(0, |b| b.id),
                    },
                    5 => Command::Arrest {
                        enemy: g.enemies().get(r.below(g.enemies().len().max(1))).map_or(0, |e| e.id),
                    },
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

// ---------------------------------------------------------------- IT, guns and power

/// Play waves with the balanced bot until `wave` waves are done.
fn bot_game_until(seed: u64, wave: u32) -> (Game, Bot) {
    let mut g = Game::new(seed);
    let mut bot = Bot::balanced(g.map());
    while g.wave() < wave && !g.is_over() {
        bot.take_turn(&mut g);
        g.run_wave(WAVE_CAP);
        g.drain_events();
    }
    (g, bot)
}

#[test]
fn supply_arms_rebels_only_after_joining() {
    assert_eq!(supply_odds(SUPPLY_JOINS_WAVE - 1), (0.0, 0.0));
    let (g6, t6) = supply_odds(SUPPLY_JOINS_WAVE);
    let (g20, t20) = supply_odds(MAX_WAVES);
    assert!(g6 > 0.0 && t6 > 0.0 && g20 > g6 && t20 > t6, "Supply gives more as the war goes on");

    let mut g = Game::new(4);
    let mut bot = Bot::balanced(g.map());
    let mut joined = 0;
    let mut equipped = 0;
    while g.wave() < SUPPLY_JOINS_WAVE + 4 && !g.is_over() {
        bot.take_turn(&mut g);
        while g.phase() == Phase::Wave {
            step_checked(&mut g);
            for e in g.drain_events() {
                match e {
                    Event::SupplyJoined => {
                        joined += 1;
                        assert_eq!(g.wave(), SUPPLY_JOINS_WAVE);
                    }
                    Event::Equipped { id, pos, gun, tools } => {
                        equipped += 1;
                        assert!(g.wave() >= SUPPLY_JOINS_WAVE, "armed before Supply joined");
                        assert!(gun || tools);
                        assert!(pos.distance(map::supply_center()) <= map::SUPPLY_REACH + 1e-4, "equipped away from depot");
                        let e = g.enemies().iter().find(|e| e.id == id).unwrap();
                        assert_eq!((e.armed, e.tools), (e.armed || gun, e.tools || tools));
                    }
                    _ => {}
                }
            }
            // Nobody below the depot carries anything.
            for e in g.enemies() {
                if !e.supplied {
                    assert!(!e.armed && !e.tools);
                }
            }
        }
    }
    assert_eq!(joined, 1);
    assert!(equipped > 5, "only {equipped} rebels equipped");
    assert!(g.totals().armed + g.totals().tooled >= equipped);
}

#[test]
fn armed_rebels_shoot_towers_and_towers_are_repaired() {
    let (mut g, _) = bot_game_until(3, MAX_WAVES - 2);
    assert!(!g.is_over());
    g.send_wave().unwrap();
    let (mut shots, mut destroyed) = (0, 0);
    let lost_before = g.totals().towers_lost;
    while g.phase() == Phase::Wave {
        step_checked(&mut g);
        for e in g.drain_events() {
            match e {
                Event::EnemyShot { enemy, from, to, .. } => {
                    shots += 1;
                    assert!(from.distance(to) <= rebel_gun(g.wave()).range + 1e-4);
                    if let Some(e) = g.enemies().iter().find(|e| e.id == enemy) {
                        assert!(e.armed, "unarmed rebel fired");
                    }
                }
                Event::TowerDestroyed { .. } => destroyed += 1,
                _ => {}
            }
        }
    }
    assert!(shots > 0, "armed rebels never fired");
    assert_eq!(lost_before + destroyed, g.totals().towers_lost);
    assert!(g.towers().iter().all(|t| t.hp == t.max_hp), "towers repaired between waves");
}

#[test]
fn barrier_rules() {
    let mut g = Game::new(0);
    let supply_row = map::SUPPLY_ROOM[0].1;
    // Find stair tiles above and below Supply.
    let spot = |above: bool| {
        (1..map::ROWS)
            .filter(|&r| (r < supply_row) == above)
            .flat_map(|r| (0..map::COLS).map(move |c| (c, r)))
            .find(|&(c, r)| g.map().tile(c, r) == Tile::Stairs)
            .unwrap()
    };
    let below = spot(false);
    assert_eq!(g.place_barrier(below.0, below.1), Err(CommandError::BarrierNotAllowed));
    assert_eq!(g.place_barrier(2, 20), Err(CommandError::BarrierNotAllowed), "floor, not stair");
    assert_eq!(g.place_barrier(-1, 3), Err(CommandError::OutOfBounds));
    let (c, r) = (4, 20);
    assert_eq!(g.map().tile(c, r), Tile::Stairs);
    let id = g.place_barrier(c, r).unwrap();
    assert_eq!(g.chits(), START_CHITS - BARRIER_COST);
    assert_eq!(g.place_barrier(c, r), Err(CommandError::Occupied));
    assert_eq!(g.place(TowerKind::Deputy, c, r), Err(CommandError::NotBuildable));
    let refund = g.remove_barrier(id).unwrap();
    assert_eq!(refund, (BARRIER_COST as f32 * SELL_REFUND) as u32);
    assert_eq!(g.remove_barrier(id), Err(CommandError::NoSuchBarrier));
    g.validate().unwrap();
}

#[test]
fn barrier_holds_rebels_until_broken() {
    let mut g = Game::new(0);
    let id = g.place_barrier(4, 20).unwrap();
    let wall = g.barrier(id).unwrap().dist;
    g.send_wave().unwrap();
    let mut blocked_seen = false;
    let mut broken_at = None;
    while g.phase() == Phase::Wave {
        let before: Vec<(u32, f32)> = g.enemies().iter().map(|e| (e.id, e.dist)).collect();
        step_checked(&mut g);
        let intact = !g.barrier(id).unwrap().broken;
        for e in g.enemies() {
            if let Some(&(_, d0)) = before.iter().find(|(i, _)| *i == e.id)
                && d0 < wall
                && intact
            {
                assert!(e.dist < wall, "rebel {} walked through an intact barrier", e.id);
            }
            blocked_seen |= e.blocked;
        }
        for ev in g.drain_events() {
            if let Event::BarrierBroken { id: b, .. } = ev {
                assert_eq!(b, id);
                broken_at = Some(g.time());
            }
        }
    }
    assert!(blocked_seen, "nobody was stopped");
    // Wave 1 rebels have no tools: bare hands take a while, but they get through eventually.
    let t = broken_at.expect("bare hands never broke the barrier");
    assert!(t > 20.0, "broke the barrier too fast with bare hands: {t}s");
    assert_eq!(g.totals().barriers_broken, 1);
    // The wreck stays put and IT rebuilds it once the wave is over.
    let b = g.barrier(id).expect("wreck kept");
    assert!(!b.broken && b.hp == b.max_hp, "rebuilt after the wave: {b:?}");
}

#[test]
fn barrier_upgrades_raise_strength() {
    let mut g = Game::new(0);
    let id = g.place_barrier(4, 20).unwrap();
    assert_eq!(g.barrier(id).unwrap().max_hp, barrier_hp(1));
    assert_eq!(g.upgrade_barrier(id), Ok(2));
    let b = g.barrier(id).unwrap();
    assert_eq!((b.level, b.hp, b.max_hp), (2, barrier_hp(2), barrier_hp(2)));
    assert_eq!(b.invested, BARRIER_COST + 50);
    // 150 - 40 - 50 = 60 left: level 3 costs 80.
    assert!(matches!(g.upgrade_barrier(id), Err(CommandError::NotEnoughChits { need: 80, .. })));
    assert_eq!(barrier_upgrade_cost(3), None, "level 3 is the top");
    assert!(barrier_hp(3) > barrier_hp(2) && barrier_hp(2) > barrier_hp(1));
    assert_eq!(g.upgrade_barrier(999), Err(CommandError::NoSuchBarrier));
    g.validate().unwrap();
}

#[test]
fn broken_barrier_stops_blocking_and_cannot_be_upgraded() {
    let mut g = Game::new(0);
    let id = g.place_barrier(4, 20).unwrap();
    g.send_wave().unwrap();
    let mut saw_broken = false;
    while g.phase() == Phase::Wave {
        let was_broken = g.barrier(id).unwrap().broken;
        step_checked(&mut g);
        // (The step that ends the wave also rebuilds it.)
        if was_broken && g.phase() == Phase::Wave {
            saw_broken = true;
            assert_eq!(g.barrier(id).unwrap().refund(), 0);
            assert_eq!(g.upgrade_barrier(id), Err(CommandError::BarrierBroken));
            assert!(g.enemies().iter().all(|e| !e.blocked), "a wreck blocked someone");
        }
    }
    assert!(saw_broken);
    let repaired = g.drain_events().into_iter().any(|e| matches!(e, Event::BarriersRepaired { rebuilt: 1 }));
    assert!(repaired, "no repair event");
}

// ---------------------------------------------------------------- judicial

#[test]
fn arrest_removes_rebel_for_a_fee() {
    let mut g = Game::new(0);
    g.send_wave().unwrap();
    while g.enemies().is_empty() {
        step_checked(&mut g);
    }
    let e = g.enemies()[0].clone();
    let cost = g.arrest_cost(&e);
    assert_eq!(cost, arrest_cost(e.kind, 1));
    let before = g.chits();
    assert_eq!(g.arrest(e.id), Ok(cost));
    assert_eq!(g.chits(), before - cost, "no bounty for arrests");
    assert!(g.enemies().iter().all(|o| o.id != e.id));
    assert_eq!(g.totals().arrests, 1);
    assert_eq!(g.arrest(e.id), Err(CommandError::NoSuchEnemy));
    assert!(g.drain_events().iter().any(|ev| matches!(ev, Event::Arrested { id, .. } if *id == e.id)));
    g.validate().unwrap();
}

#[test]
fn leaders_cost_three_times_and_broke_players_cannot_arrest() {
    for w in [1, 10, 20] {
        assert_eq!(arrest_cost(EnemyKind::Leader, w), 3 * arrest_cost(EnemyKind::Mechanic, w));
    }
    let mut g = Game::new(0);
    // Spend down to nothing on barriers.
    let spots: Vec<(i32, i32)> = (1..map::ROWS)
        .flat_map(|r| (0..map::COLS).map(move |c| (c, r)))
        .filter(|&(c, r)| g.map().barrier_allowed(c, r))
        .collect();
    for (c, r) in spots {
        let _ = g.place_barrier(c, r);
    }
    assert!(g.chits() < BARRIER_COST);
    g.send_wave().unwrap();
    while g.enemies().is_empty() {
        step_checked(&mut g);
    }
    // Keep calling Judicial until the money runs out; the failed call must cost nothing.
    loop {
        while g.enemies().is_empty() {
            step_checked(&mut g);
        }
        let id = g.enemies()[0].id;
        let before = g.chits();
        match g.arrest(id) {
            Ok(_) => continue,
            Err(e) => {
                assert!(matches!(e, CommandError::NotEnoughChits { .. }), "{e:?}");
                assert_eq!(g.chits(), before);
                break;
            }
        }
    }
}

#[test]
fn tools_break_barriers_much_faster_than_fists() {
    assert!(barrier_dps(true, 6) >= barrier_dps(false, 6) * 5.0);
    assert!(BARRIER_HP / barrier_dps(true, 10) < 8.0, "a tooled rebel breaks a barrier in seconds");
}

#[test]
fn it_collapse_blinds_lower_cameras_until_reboot() {
    let mut g = Game::new(0);
    let mut collapsed_at = None;
    let mut restored = false;
    while !g.is_over() {
        g.send_wave().unwrap();
        while g.phase() == Phase::Wave {
            step_checked(&mut g);
            for e in g.drain_events() {
                match e {
                    Event::ItCollapsed { seconds } => {
                        assert_eq!(seconds, IT_DOWN_TIME);
                        collapsed_at = Some(g.time());
                        assert!(g.cameras_down());
                        assert_eq!(g.it_hp(), 0.0);
                        assert!(g.is_row_dark(map::SECTIONS[1].1) && g.is_row_dark(map::ROWS - 1));
                        assert!(!g.is_row_dark(map::SECTIONS[0].2), "Up Top is still watched");
                    }
                    Event::ItRestored => {
                        restored = true;
                        assert!(!g.cameras_down());
                        assert_eq!(g.it_hp(), IT_HP);
                    }
                    _ => {}
                }
            }
        }
        if g.phase() == Phase::Building {
            assert!(!g.cameras_down() && g.it_hp() == IT_HP, "IT rebooted between waves");
        }
    }
    let t = collapsed_at.expect("undefended IT never collapsed");
    assert!(restored || g.phase() == Phase::Lost, "collapsed at {t}s, never restored");
}

#[test]
fn power_cuts_only_in_final_waves_and_lift_between_waves() {
    let mut g = Game::new(9);
    let mut bot = Bot::balanced(g.map());
    let mut cuts = 0;
    while !g.is_over() {
        bot.take_turn(&mut g);
        while g.phase() == Phase::Wave {
            step_checked(&mut g);
            for e in g.drain_events() {
                if let Event::PowerCut { first, last, seconds } = e {
                    cuts += 1;
                    assert!(g.wave() >= POWER_CUT_WAVE, "power cut in wave {}", g.wave());
                    assert!(seconds > 0.0 && first <= last);
                    if g.phase() == Phase::Wave {
                        assert!((first..=last).all(|r| g.is_row_dark(r)));
                    }
                    assert!(map::SECTIONS.iter().any(|s| (s.1, s.2) == (first, last)), "cut a whole section");
                    assert!(g.blackouts().len() <= 1, "one section at a time");
                }
            }
            if g.wave() < POWER_CUT_WAVE {
                assert!(g.blackouts().is_empty());
            }
        }
        if g.phase() != Phase::Lost {
            assert!(g.blackouts().is_empty(), "power restored after the wave");
        }
    }
    assert!(g.wave() < POWER_CUT_WAVE || cuts > 0, "no power cuts in the final waves");
}
