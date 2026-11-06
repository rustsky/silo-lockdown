//! ASCII dump of tile classes: `cargo run -p silo_core --example map_dump`
use silo_core::map::{COLS, ROWS};
use silo_core::{Map, Tile};

fn main() {
    let m = Map::new();
    println!("path length {:.1}", m.length());
    for r in 0..ROWS {
        let row: String = (0..COLS)
            .map(|c| match m.tile(c, r) {
                Tile::Outside => '~',
                Tile::Airlock => 'A',
                Tile::Rock => '#',
                Tile::Stairs => 'S',
                Tile::Shaft => '.',
                Tile::Floor => '_',
                Tile::OffMap => ' ',
            })
            .collect();
        println!("{r:>2} {row}");
    }
}
