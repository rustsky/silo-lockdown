//! Per-wave trace of a bot game: `cargo run -p silo_core --example trace --release`
use silo_core::bot::Bot;
use silo_core::{Event, Game};

fn main() {
    let mut g = Game::new(0);
    let mut bot = Bot::balanced(g.map());
    while !g.is_over() {
        bot.take_turn(&mut g);
        g.run_wave(30 * 600);
        let mut lost = vec![];
        let (mut destroyed, mut it_falls, mut cuts) = (0, 0, 0);
        for e in g.drain_events() {
            match e {
                Event::Breach { kind, .. } => lost.push(kind.name()),
                Event::TowerDestroyed { .. } => destroyed += 1,
                Event::ItCollapsed { .. } => it_falls += 1,
                Event::PowerCut { .. } => cuts += 1,
                _ => {}
            }
        }
        let lv: Vec<_> = g.towers().iter().map(|t| format!("{}{}", &t.kind.name()[..1], t.level)).collect();
        println!("wave {:>2} seals {:>2} chits {:>4} t={:>6.0}s leaks {:?} destroyed {destroyed} IT-falls {it_falls} cuts {cuts}\n         towers {}", g.wave(), g.seals(), g.chits(), g.time(), lost, lv.join(" "));
    }
    println!("{:?}", g.phase());
}
