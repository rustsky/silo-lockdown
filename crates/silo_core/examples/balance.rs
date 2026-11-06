//! Balance report: `cargo run -p silo_core --example balance --release`
use silo_core::bot::{Bot, simulate};
use silo_core::{Game, Phase, TowerKind::*};

fn main() {
    let plans: Vec<(&str, Vec<_>)> = vec![
        ("balanced", vec![Deputy, Deputy, Jammer, Gas, Deputy, Marksman, Gas, Jammer, Marksman, Deputy]),
        ("deputy-only", vec![Deputy]),
        ("gas-only", vec![Gas]),
        ("marksman-only", vec![Marksman]),
        ("jammer+deputy", vec![Deputy, Jammer]),
    ];
    let map = Game::new(0).map().clone();
    for (name, plan) in plans {
        let (mut wins, mut waves, mut seals) = (0, 0, 0);
        let n = 20;
        for seed in 0..n {
            let r = simulate(seed, &mut Bot::with_plan(&map, plan.clone()));
            wins += (r.phase == Phase::Won) as u32;
            waves += r.wave;
            seals += r.seals;
        }
        println!(
            "{name:>14}: won {wins:>2}/{n}  avg wave {:>5.1}  avg seals left {:>4.1}",
            waves as f32 / n as f32,
            seals as f32 / n as f32
        );
    }
}
