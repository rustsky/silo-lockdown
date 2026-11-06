//! Silo: Lockdown — graphical frontend (macroquad): desktop, web (WASM), Android, iOS.
//! All game rules live in `silo_core`; this crate only renders and handles input.

mod app;
mod fx;
mod light;
mod render;
mod ui;

use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Silo: Lockdown".to_owned(),
        window_width: 588,
        window_height: 854,
        high_dpi: true,
        window_resizable: true,
        sample_count: 4,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut app = app::App::new();
    loop {
        app.frame();
        if app.should_quit() {
            break;
        }
        next_frame().await;
    }
}
