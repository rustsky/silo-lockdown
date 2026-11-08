//! Silo: Lockdown — graphical frontend (macroquad): desktop, web (WASM), Android, iOS.
//! All game rules live in `silo_core`; this crate only renders and handles input.

mod app;
mod cleaning;
mod fx;
mod judicial;
mod light;
mod music;
mod render;
mod safeguard;
mod sfx;
mod stats;
mod synth;
mod ui;
mod uprising;
mod victory;

use macroquad::experimental::coroutines::start_coroutine;
use macroquad::prelude::*;

fn window_conf() -> Conf {
    // SILO_SHOT_WINDOW=W,H: another window size for screenshots (the game scales to fit).
    let size: Vec<i32> = std::env::var("SILO_SHOT_WINDOW")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_default();
    let (width, height) = match size[..] {
        [w, h] => (w, h),
        _ => (588, 854),
    };
    Conf {
        window_title: "Silo: Lockdown".to_owned(),
        window_width: width,
        window_height: height,
        high_dpi: true,
        window_resizable: true,
        sample_count: 4,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut app = app::App::new();
    // The soundtrack and effects are synthesised in the background (no audio files); music fades in once
    // ready; on the web, decoding finishes asynchronously, so never block the game on it.
    let mut loading = Some(start_coroutine(async {
        (music::Music::load().await, sfx::SoundBank::load().await)
    }));
    loop {
        if let Some(c) = loading.as_ref().filter(|c| c.is_done()) {
            if let Some((music, effects)) = c.retrieve() {
                app.set_audio(music, effects);
            }
            loading = None;
        }
        app.frame();
        if app.should_quit() {
            break;
        }
        next_frame().await;
    }
}
