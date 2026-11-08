//! Game loop glue: input, fixed-step simulation, events -> effects, HUD and command panel.

use crate::cleaning::Cleaning;
use crate::fx::{Fx, Kind, PKind};
use crate::judicial::Judicial;
use crate::uprising::Uprising;
use crate::light::Lighting;
use crate::music::Music;
use crate::render::{self, *};
use crate::sfx::{Sfx, SoundBank};
use crate::ui::*;
use macroquad::prelude::*;
use silo_core as sc;
use silo_core::bot::Bot;
use silo_core::map::{SECTIONS, section_view_top};
use silo_core::stats::{BARRIER_COST, BARRIER_MAX_LEVEL, MAX_TOWER_LEVEL, MAX_WAVES, START_SEALS};
use silo_core::{Event, Game, Phase, TICK, TowerKind};

/// A press that started in the field: a short press is a tap, a vertical drag rolls the view.
struct Drag {
    start: Vec2,
    scroll0: f32,
    moved: bool,
}

const SECTION_COUNT: usize = SECTIONS.len();

fn view_scroll(view: usize) -> f32 {
    section_view_top(view) as f32 * TILE
}

fn max_scroll() -> f32 {
    view_scroll(SECTION_COUNT - 1)
}

/// Overview column on the right wall: up arrow, minimap, down arrow (screen pixels).
fn nav_rects() -> (Rect, Rect, Rect) {
    let x = W - TILE + 5.0;
    let w = TILE - 10.0;
    let up = Rect::new(x, HUD_H + 6.0, w, 40.0);
    let down = Rect::new(x, PANEL_Y - 46.0, w, 40.0);
    let map = Rect::new(x, up.y + up.h + 6.0, w, down.y - up.y - up.h - 12.0);
    (up, map, down)
}

/// What a tap on an empty tile builds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Tower(TowerKind),
    Barrier,
    /// Sims' agents: the next rebel tapped is arrested on the spot.
    Arrest,
}

#[derive(PartialEq, Eq)]
enum Screen {
    Title,
    Playing,
}

pub struct App {
    game: Game,
    bot: Bot,
    autopilot: bool,
    screen: Screen,
    build: Option<Tool>,
    selected: Option<sc::Id>,
    selected_barrier: Option<sc::Id>,
    /// Rebel picked for arrest by Judicial.
    selected_enemy: Option<sc::Id>,
    /// Arrests in progress (Sims' agents marching rebels off).
    judicial: Judicial,
    /// Seconds left of the crowd's chant: the music ducks under it meanwhile.
    chant_left: f32,
    /// The rally at the generator hall that opens each wave.
    uprising: Uprising,
    /// Static placement rules per tile: [Deputy, Jammer, Gas, Marksman, Barrier][row][col]
    /// (ground type and reach; occupancy and cost are checked live).
    spots: Vec<Vec<Vec<bool>>>,
    /// The "Sims' agents can arrest rebels" hint was shown this game.
    sims_hinted: bool,
    speed: u32,
    paused: bool,
    acc: f32,
    time: f32,
    fx: Fx,
    /// Downed leaders being sent out to clean the sensor.
    cleaning: Cleaning,
    /// Accumulator for the gas launchers' idle venting.
    vent_acc: f32,
    /// Section the view is rolling to (0 = Up Top .. 2 = Down Deep).
    view: usize,
    /// Current vertical scroll of the field in world pixels (animates toward `view`).
    scroll: f32,
    /// Section name flashed at the top of the field after rolling, with time left.
    view_label: f32,
    drag: Option<Drag>,
    wheel: f32,
    wheel_cooldown: f32,
    banner: Option<(String, String, f32)>,
    /// Wave whose first power cut was announced (later cuts that wave stay quiet).
    power_announced: u32,
    decor: Decor,
    light: Lighting,
    music: Option<Music>,
    sfx: Option<SoundBank>,
    /// 0 = music and effects, 1 = effects only, 2 = silent.
    audio_mode: u8,
    /// Screenshot mode (SILO_SHOT=path.png): autoplay and capture after N frames.
    shot: Option<(String, u32)>,
    frame: u32,
    quit: bool,
}

fn seed() -> u64 {
    (macroquad::miniquad::date::now() * 1000.0) as u64
}

impl App {
    pub fn new() -> Self {
        let game = Game::new(seed());
        let bot = Bot::balanced(game.map());
        let decor = Decor::new(game.map());
        let shot = std::env::var("SILO_SHOT").ok().map(|p| {
            let frames = std::env::var("SILO_SHOT_FRAME")
                .ok()
                .and_then(|f| f.parse().ok())
                .unwrap_or(900);
            (p, frames)
        });
        let title = shot.is_none() || std::env::var("SILO_SHOT_TITLE").is_ok();
        let mut app = Self {
            game,
            bot,
            autopilot: true,
            screen: Screen::Title,
            build: None,
            selected: None,
            selected_barrier: None,
            selected_enemy: None,
            judicial: Judicial::default(),
            uprising: Uprising::default(),
            chant_left: 0.0,
            sims_hinted: false,
            spots: Vec::new(),
            speed: 2,
            paused: false,
            acc: 0.0,
            time: 0.0,
            fx: Fx::default(),
            cleaning: Cleaning::default(),
            vent_acc: 0.0,
            view: 0,
            scroll: 0.0,
            view_label: 0.0,
            drag: None,
            wheel: 0.0,
            wheel_cooldown: 0.0,
            banner: None,
            power_announced: 0,
            decor,
            light: Lighting::new(),
            music: None,
            sfx: None,
            audio_mode: 0,
            shot,
            frame: 0,
            quit: false,
        };
        app.spots = placement_spots(app.game.map());
        if !title {
            app.start();
            app.autopilot = true;
            app.speed = 4;
            // SILO_SHOT_SKIP=N: let the AI play N waves instantly before the capture starts.
            let skip: u32 = std::env::var("SILO_SHOT_SKIP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            while app.game.wave() < skip && !app.game.is_over() {
                app.bot.take_turn(&mut app.game);
                app.game.run_wave(30 * 60 * 10);
                app.game.drain_events();
            }
            // SILO_SHOT_BARRIERS=1: barricade a few stair tiles in Mids for the capture.
            if std::env::var("SILO_SHOT_BARRIERS").is_ok() {
                // Levels 3, 2, 1, 1 (upgrades only if the chits allow).
                for (i, (c, r)) in [(4, 16), (7, 18), (4, 20), (7, 22)].into_iter().enumerate() {
                    if let Ok(id) = app.game.place_barrier(c, r) {
                        for _ in 0..[2, 1, 0, 0][i] {
                            let _ = app.game.upgrade_barrier(id);
                        }
                    }
                }
            }
            // SILO_SHOT_SELECT=KIND,C,R: build a tower (0=deputy..3=marksman) and select it.
            if let Ok(v) = std::env::var("SILO_SHOT_SELECT") {
                let n: Vec<i32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if let [k, c, r] = n[..]
                    && let Ok(id) = app.game.place(TowerKind::ALL[k as usize % 4], c, r)
                {
                    app.selected = Some(id);
                    app.build = None;
                }
            }
            // SILO_SHOT_TOOL=I (0-3 towers, 4 barrier) [+ SILO_SHOT_TAP=C,R: a tap on that tile].
            if let Some(i) = std::env::var("SILO_SHOT_TOOL").ok().and_then(|v| v.parse::<usize>().ok()) {
                app.build = Some(if i < 4 { Tool::Tower(TowerKind::ALL[i]) } else { Tool::Barrier });
                if let Ok(v) = std::env::var("SILO_SHOT_TAP") {
                    let n: Vec<i32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                    if let [c, r] = n[..] {
                        app.field_tap(px(sc::map::tile_center(c, r)));
                    }
                }
            }
            // SILO_SHOT_CLEAN=D[,D..]: send cleaners from D tiles short of the airlock.
            if let Ok(v) = std::env::var("SILO_SHOT_CLEAN") {
                let len = app.game.map().length();
                for d in v.split(',').filter_map(|d| d.parse::<f32>().ok()) {
                    app.cleaning.send(app.game.map(), (len - d).max(0.0), true);
                }
            }
            let view = std::env::var("SILO_SHOT_VIEW")
                .ok()
                .and_then(|v| v.parse().ok());
            if let Some(v) = view {
                app.view = usize::min(v, SECTION_COUNT - 1);
                app.scroll = view_scroll(app.view);
            }
        }
        app
    }

    pub fn set_audio(&mut self, music: Option<Music>, sfx: Option<SoundBank>) {
        self.music = music;
        self.sfx = sfx;
        // Screenshot runs stay silent.
        if self.shot.is_some() {
            self.audio_mode = 2;
        }
        self.apply_audio_mode();
    }

    /// Cycle: music + effects -> effects only -> silent.
    fn toggle_music(&mut self) {
        self.audio_mode = (self.audio_mode + 1) % 3;
        self.apply_audio_mode();
    }

    fn apply_audio_mode(&mut self) {
        if let Some(m) = &mut self.music {
            m.muted = self.audio_mode != 0;
        }
        if let Some(s) = &mut self.sfx {
            s.muted = self.audio_mode == 2;
        }
    }

    /// Play an effect, quieter when it happens outside the visible part of the silo.
    fn sound(&mut self, kind: Sfx, at: Option<sc::Vec2>) {
        let mut vol = if self.screen == Screen::Playing {
            1.0
        } else {
            0.25
        };
        if let Some(p) = at {
            let y = HUD_H + p.y * TILE;
            if y < self.scroll + HUD_H - TILE || y > self.scroll + PANEL_Y + TILE {
                vol *= 0.35;
            }
        }
        if let Some(s) = &mut self.sfx {
            s.play(kind, vol);
        }
    }

    fn event_sound(&mut self, ev: &Event) {
        use sc::map::{it_center, tile_center};
        let tower_pos = |g: &Game, id: sc::Id| g.tower(id).map(|t| t.pos);
        match ev {
            Event::Shot { kind, from, .. } => {
                let s = match kind {
                    TowerKind::Deputy => Sfx::Deputy,
                    TowerKind::Gas => Sfx::Gas,
                    TowerKind::Marksman => Sfx::Marksman,
                    TowerKind::Jammer => Sfx::Jammer,
                };
                self.sound(s, Some(*from));
            }
            Event::Pulse { pos, .. } => self.sound(Sfx::Jammer, Some(*pos)),
            Event::EnemyShot { from, .. } => self.sound(Sfx::RebelShot, Some(*from)),
            Event::Explosion { pos, .. } => self.sound(Sfx::Explosion, Some(*pos)),
            Event::EnemyKilled { pos, .. } => {
                self.sound(Sfx::Kill, Some(*pos));
                self.sound(Sfx::Coin, Some(*pos));
            }
            Event::Breach { .. } => self.sound(Sfx::Breach, None),
            Event::TowerPlaced { col, row, .. } | Event::BarrierPlaced { col, row, .. } => {
                self.sound(Sfx::Place, Some(tile_center(*col, *row)))
            }
            Event::TowerUpgraded { id, .. } => {
                let p = tower_pos(&self.game, *id);
                self.sound(Sfx::Upgrade, p);
            }
            Event::TowerSold { .. } | Event::BarrierRemoved { .. } => self.sound(Sfx::Sell, None),
            Event::BarrierBroken { pos, .. } => self.sound(Sfx::BarrierBroken, Some(*pos)),
            Event::BarrierUpgraded { id, .. } => {
                let p = self.game.barrier(*id).map(|b| b.pos);
                self.sound(Sfx::Upgrade, p);
            }
            Event::BarriersRepaired { .. } => {}
            Event::Arrested { pos, .. } => self.sound(Sfx::Equip, Some(*pos)),
            Event::TowerDestroyed { pos, .. } => self.sound(Sfx::TowerDestroyed, Some(*pos)),
            Event::ItCollapsed { .. } => self.sound(Sfx::ItDown, None),
            Event::ItRestored => self.sound(Sfx::ItUp, Some(it_center())),
            Event::PowerCut { .. } => self.sound(Sfx::PowerCut, None),
            Event::WaveStarted { .. } | Event::SupplyJoined => self.sound(Sfx::Horn, None),
            Event::WaveCleared { .. } | Event::Won => self.sound(Sfx::Cleared, None),
            Event::Equipped { pos, .. } => self.sound(Sfx::Equip, Some(*pos)),
            Event::Lost => {
                self.sound(Sfx::TowerDestroyed, None);
                self.sound(Sfx::Breach, None);
            }
            Event::EnemySpawned { .. } => {}
        }
    }

    /// Calm between waves, driving during them, subdued once the airlock is open.
    fn update_music(&mut self, dt: f32) {
        let playing = self.screen == Screen::Playing;
        let (intensity, volume) = match self.game.phase() {
            Phase::Wave if playing => (1.0, 1.0),
            Phase::Lost if playing => (0.0, 0.3),
            _ if playing => (0.0, 1.0),
            _ => (0.0, 0.7), // title screen
        };
        // The music ducks under the crowd's chant and swells back as it ends.
        self.chant_left = (self.chant_left - dt).max(0.0);
        let duck = if self.chant_left > 0.0 { 0.3 } else { 1.0 };
        if let Some(m) = &mut self.music {
            m.update(dt, intensity, volume, duck);
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    fn start(&mut self) {
        self.game = Game::new(seed());
        self.bot = Bot::balanced(self.game.map());
        self.screen = Screen::Playing;
        self.autopilot = false;
        self.build = Some(Tool::Tower(TowerKind::Deputy));
        self.selected = None;
        self.selected_barrier = None;
        self.selected_enemy = None;
        self.judicial.clear();
        self.uprising.clear();
        self.sims_hinted = false;
        self.speed = 1;
        self.paused = false;
        self.acc = 0.0;
        self.fx.clear();
        self.cleaning.clear();
        // Start where the uprising starts.
        self.view = SECTION_COUNT - 1;
        self.drag = None;
        self.view_label = 2.0;
        self.banner = Some((
            "YOU ARE IT".into(),
            "Mechanical is rising. Build on the levels and catwalks, then send the wave. \
             Drag, scroll or use the arrows to roll between Up Top, Mids and Down Deep."
                .into(),
            6.0,
        ));
    }

    pub fn frame(&mut self) {
        let dt = get_frame_time().min(0.1);
        self.time += dt;
        self.frame += 1;
        // SILO_SHOT_ARREST=N: at frame N pick the rebel nearest the middle of the view, and
        // arrest them 20 frames later (unless SILO_SHOT_ARREST_HOLD is set: panel shot).
        if self.shot.is_some()
            && let Some(n) = std::env::var("SILO_SHOT_ARREST")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
        {
            if self.frame == n {
                let mid = self.scroll + HUD_H + FIELD_H / 2.0;
                self.selected_enemy = self
                    .game
                    .enemies()
                    .iter()
                    .filter(|e| self.game.is_visible(e.pos))
                    .min_by(|a, b| {
                        (px(a.pos).y - mid).abs().total_cmp(&(px(b.pos).y - mid).abs())
                    })
                    .map(|e| e.id);
            }
            if self.frame == n + 20 && std::env::var("SILO_SHOT_ARREST_HOLD").is_err() {
                self.arrest_selected();
            }
        }

        // Letterboxed logical canvas of W x H.
        let (sw, sh) = (screen_width(), screen_height());
        let scale = (sw / W).min(sh / H);
        set_scale(scale);
        let (vw, vh) = (sw / scale, sh / scale);
        let shake = if self.fx.shake > 0.0 {
            vec2(rand::gen_range(-4.0, 4.0), rand::gen_range(-4.0, 4.0)) * self.fx.shake * 2.0
        } else {
            Vec2::ZERO
        };
        // Note: for on-screen cameras macroquad 0.4 already flips Y internally, so a positive
        // y zoom gives a y-down world (from_display_rect would render upside down here).
        let cam = Camera2D {
            target: vec2(W / 2.0, H / 2.0) + shake,
            zoom: vec2(2.0 / vw, 2.0 / vh),
            ..Default::default()
        };
        let pointer = cam.screen_to_world(mouse_position().into());
        let tap = is_mouse_button_pressed(MouseButton::Left)
            .then_some(pointer)
            .filter(|_| self.shot.is_none());
        let mut ui = Ui {
            pointer,
            tap,
            consumed: false,
        };

        // Screenshot runs are hands-off: stray input must not move the view.
        if self.shot.is_none() {
            self.keys();
        }
        self.simulate(dt);
        self.fx.update(dt);
        self.update_gas_and_cleaning(dt);
        self.update_scroll(dt);
        self.update_music(dt);
        // The world camera is the screen camera rolled down by the current scroll.
        let world_cam = Camera2D {
            target: cam.target + vec2(0.0, self.scroll),
            zoom: cam.zoom,
            ..Default::default()
        };
        let world_pointer = pointer + vec2(0.0, self.scroll);
        if let Some((_, _, t)) = &mut self.banner {
            *t -= dt;
            if *t <= 0.0 {
                self.banner = None;
            }
        }

        clear_background(rgb(8, 8, 10));
        set_camera(&world_cam);
        let vis = Vis {
            y0: self.scroll + HUD_H,
            y1: self.scroll + PANEL_Y,
        };
        render::world(
            &self.game,
            &self.decor,
            self.time,
            self.fx.red_flash,
            &mut self.light,
            vis,
        );
        // Light the scene. Nothing shines out of rooms IT can't see.
        self.fx.lights(&mut self.light);
        let g = &self.game;
        self.light.lights.retain(|l| {
            vis.contains(l.pos.y, l.radius)
                && (l.pos.y < HUD_H + TILE
                    || g.is_visible(sc::vec2(l.pos.x / TILE, (l.pos.y - HUD_H) / TILE)))
        });
        self.light.cones.retain(|c| {
            vis.contains(c.apex.y, c.height)
                && g.is_visible(sc::vec2(c.apex.x / TILE, (c.apex.y - HUD_H) / TILE))
        });
        let view = Rect::new(W / 2.0 - vw / 2.0, H / 2.0 - vh / 2.0, vw, vh);
        let field = Rect::new(0.0, HUD_H, W, FIELD_H);
        self.light.apply(
            &world_cam,
            &cam,
            view,
            field,
            Color::new(0.3, 0.31, 0.38, 1.0),
        );
        set_camera(&world_cam);
        self.cleaning.draw_sensor(self.time);
        self.fx.draw();
        self.uprising.draw(&self.game, self.time);
        self.judicial.draw(&self.game, self.time);
        self.cleaning.draw(&self.game, self.time);
        self.field_overlays(&ui, world_pointer);
        render::darkness(&self.game, self.time);
        set_camera(&cam);
        // The world is taller than the field: cover what spills over the HUD and panel.
        draw_rectangle(0.0, -vh, W, vh + HUD_H, rgb(8, 8, 10));
        draw_rectangle(0.0, PANEL_Y, W, vh + PANEL_H, rgb(8, 8, 10));
        vignette();
        self.draw_view_label();
        if self.fx.red_flash > 0.0 {
            draw_rectangle(
                0.0,
                HUD_H,
                W,
                FIELD_H,
                rgba(255, 0, 0, (self.fx.red_flash * 120.0) as u8),
            );
        }
        self.draw_banner();
        self.fx.draw_toast(W / 2.0, PANEL_Y - 24.0);

        match self.screen {
            Screen::Title => self.title(&mut ui),
            Screen::Playing => {
                self.hud(&mut ui);
                self.panel(&mut ui);
                self.navigator(&mut ui);
                if self.game.is_over() {
                    self.game_over(&mut ui);
                }
                self.field_input(&mut ui);
            }
        }
        // Mask anything drawn outside the canvas (letterbox).
        let bars = rgb(8, 8, 10);
        draw_rectangle(-vw, -vh, vw, vh * 3.0, bars);
        draw_rectangle(W, -vh, vw, vh * 3.0, bars);
        draw_rectangle(-vw, -vh, vw * 3.0, vh, bars);
        draw_rectangle(-vw, H, vw * 3.0, vh, bars);
        set_default_camera();

        self.maybe_screenshot();
    }

    fn maybe_screenshot(&mut self) {
        if let Some((path, at)) = &self.shot
            && self.frame >= *at
        {
            {
                // Saved exactly as read back (no flip), so it matches what the window shows.
                let mut img = get_screen_data();
                for px in img.bytes.as_chunks_mut::<4>().0 {
                    px[3] = 255;
                }
                img.export_png(path);
                self.quit = true;
            }
        }
    }

    // ------------------------------------------------------------ simulation

    /// Gas trails behind canisters in flight, gas launchers venting while idle, and the
    /// cleaners' walk (both run on game time, so they freeze when paused).
    fn update_gas_and_cleaning(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        let gdt = dt * self.speed as f32;
        let cues = self.uprising.update(gdt);
        for p in cues.sparks {
            self.fx.burst(PKind::Spark, p, 7, 150.0, rgb(255, 210, 120));
        }
        if let Some((head, coils, slogan)) = cues.shout
            && self.game.is_visible(sc::map::power_center())
        {
            // "WE WANT THE TRUTH!": a red shockwave, the generator surges, leaflets fly.
            // Sound, silence and shake only in a real game, not the title screen's demo.
            let playing = self.screen == Screen::Playing;
            if playing {
                self.fx.shake = self.fx.shake.max(0.3);
            }
            self.fx.add(head, 0.7, Kind::Ring { max_r: 110.0, color: rgb(255, 70, 40) });
            self.fx.add(head, 1.0, Kind::Ring { max_r: 170.0, color: rgb(255, 140, 60) });
            self.fx.burst(PKind::Spark, coils, 26, 260.0, rgb(255, 230, 150));
            self.fx.burst(PKind::Ember, head, 18, 120.0, rgb(255, 90, 50));
            for _ in 0..16 {
                let v = vec2(rand::gen_range(-90.0, 140.0), rand::gen_range(-260.0, -140.0));
                let shade_k = rand::gen_range(0.8, 1.0);
                let col = Color::new(0.95 * shade_k, 0.93 * shade_k, 0.86 * shade_k, 1.0);
                self.fx
                    .particle(PKind::Paper, head + vec2(rand::gen_range(0.0, 60.0), 10.0), v, rand::gen_range(2.2, 3.4), col);
            }
            if playing {
                // The chant for this wave's slogan.
                if let Some(s) = &mut self.sfx {
                    s.play_variant(Sfx::Roar, slogan, 1.0);
                }
                // Duck the music under the chant (the last word fades out by ~1.5 s).
                self.chant_left = 1.5;
            }
        }
        for d in self.judicial.update(gdt) {
            self.cleaning.send(self.game.map(), d, true);
        }
        for w in self.cleaning.update(self.game.map(), gdt) {
            self.fx.add(
                w - vec2(0.0, 6.0),
                1.4,
                Kind::Text {
                    text: "LENS CLEAN".into(),
                    color: rgb(200, 230, 255),
                },
            );
        }
        let g = &self.game;
        let trail = (gdt * 50.0).min(4.0);
        for p in g.projectiles() {
            if p.kind != sc::ProjectileKind::Shell || !g.is_visible(p.pos) {
                continue;
            }
            if rand::gen_range(0.0, 1.0) < trail {
                let v = vec2(rand::gen_range(-12.0, 12.0), rand::gen_range(-14.0, 4.0));
                self.fx
                    .gas(px(p.pos), v, rand::gen_range(0.5, 0.9), rand::gen_range(2.5, 4.0), gas_color());
            }
        }
        // Idle venting: a lazy wisp from the tube and the canister rack now and then.
        self.vent_acc += gdt;
        if self.vent_acc >= 0.12 {
            self.vent_acc = 0.0;
            for t in g.towers() {
                if t.kind != TowerKind::Gas || !g.is_visible(t.pos) || rand::gen_range(0, 3) != 0 {
                    continue;
                }
                let c = tower_anchor(g.map(), t);
                let aim = t.aim.map(px);
                let (tip, dir) = gas_muzzle(c, aim);
                let from = if rand::gen_range(0, 2) == 0 {
                    tip
                } else {
                    let facing = if dir.x < 0.0 { -1.0 } else { 1.0 };
                    vec2(c.x - 16.0 * facing + rand::gen_range(0, 3) as f32 * 4.5 * facing, c.y - 4.0)
                };
                let v = vec2(rand::gen_range(-6.0, 6.0), rand::gen_range(-22.0, -10.0));
                let size = rand::gen_range(1.8, 3.2) + t.level as f32 * 0.4;
                self.fx.gas(from, v, rand::gen_range(1.0, 1.8), size, gas_color_faint());
            }
        }
    }

    fn simulate(&mut self, dt: f32) {
        // SILO_SHOT_NOAI=1: screenshot runs leave the silo undefended (waves still roll in).
        if self.shot.is_some()
            && std::env::var("SILO_SHOT_NOAI").is_ok()
            && self.game.phase() == Phase::Building
        {
            self.autopilot = false;
            let _ = self.game.send_wave();
        }
        let auto = self.autopilot || self.screen == Screen::Title;
        if self.screen == Screen::Title && self.game.is_over() {
            self.game = Game::new(seed()); // attract mode loops forever
        }
        if auto && self.game.phase() == Phase::Building {
            self.bot.take_turn(&mut self.game);
        }
        if !self.paused {
            self.acc += dt * self.speed as f32;
            let mut steps = 0;
            while self.acc >= TICK && steps < 16 {
                self.game.step();
                if auto && self.game.phase() == Phase::Wave && steps == 0 {
                    self.bot.spend(&mut self.game);
                }
                self.acc -= TICK;
                steps += 1;
            }
            if steps == 16 {
                self.acc = 0.0;
            }
        }
        if self
            .selected
            .is_some_and(|id| self.game.tower(id).is_none())
        {
            self.selected = None;
        }
        if self
            .selected_barrier
            .is_some_and(|id| self.game.barrier(id).is_none())
        {
            self.selected_barrier = None;
        }
        if self.selected_enemy.is_some_and(|id| {
            !self
                .game
                .enemies()
                .iter()
                .any(|e| e.id == id && e.hp > 0.0 && self.game.is_visible(e.pos))
        }) {
            self.selected_enemy = None;
        }
        for ev in self.game.drain_events() {
            self.event_sound(&ev);
            self.on_event(ev);
        }
        // Arrest mode only makes sense during a wave.
        if self.build == Some(Tool::Arrest) && self.game.phase() != Phase::Wave {
            self.build = None;
        }
        // First time rebels get close to the airlock, introduce Sims' agents.
        if !self.sims_hinted
            && self.screen == Screen::Playing
            && self.game.phase() == Phase::Wave
            && self.banner.is_none()
            && self.rebels_near_airlock()
        {
            self.sims_hinted = true;
            self.banner = Some((
                "SIMS' AGENTS".into(),
                "Rebels near the airlock! Tap one (or the SIMS' AGENTS card): Judicial \
                 arrests them for chits and sends them to cleaning."
                    .into(),
                5.0,
            ));
        }
        // Sledgehammers on barricades throw sparks.
        let hammering: Vec<sc::Vec2> = self
            .game
            .enemies()
            .iter()
            .filter(|e| e.blocked && e.tools)
            .map(|e| e.pos)
            .collect();
        for pos in hammering {
            if rand::gen_range(0.0, 1.0) < dt * 4.0 {
                self.sound(Sfx::BarrierHit, Some(pos));
                let p = px(pos) + vec2(rand::gen_range(-8.0, 8.0), -22.0);
                self.fx.burst(PKind::Spark, p, 4, 140.0, rgb(255, 210, 140));
            }
        }
    }

    fn on_event(&mut self, ev: Event) {
        let playing = self.screen == Screen::Playing;
        let airlock = vec2(6.0 * TILE, HUD_H + TILE * 0.5);
        if let Event::WaveStarted { wave, .. } = ev {
            self.uprising.start(wave);
        }
        match ev {
            Event::WaveStarted { wave, lore } if playing => {
                self.banner = Some((format!("WAVE {wave}"), lore.to_string(), 4.0));
                // Roll down to the generator hall to watch the uprising start (screenshot
                // runs that asked for a section stay there).
                let pinned = self.shot.is_some() && std::env::var("SILO_SHOT_VIEW").is_ok();
                if self.drag.is_none() && !pinned {
                    self.view = SECTION_COUNT - 1;
                }
            }
            Event::WaveCleared { wave, bonus } if playing => {
                self.banner = Some((
                    format!("WAVE {wave} HELD"),
                    format!("+{bonus} chits from the Overseer"),
                    2.5,
                ));
            }
            Event::Breach { seals_lost, .. } => {
                self.fx.shake = 0.35;
                self.fx.red_flash = 0.45;
                self.fx.add(
                    airlock + vec2(0.0, 40.0),
                    1.2,
                    Kind::Text {
                        text: format!("-{seals_lost} SEAL"),
                        color: rgb(255, 90, 70),
                    },
                );
            }
            Event::EnemyKilled {
                kind,
                pos,
                dist,
                bounty,
                ..
            } => {
                let p = px(pos);
                self.fx.add(p - vec2(0.0, 10.0), 0.5, Kind::Puff);
                self.fx.burst(
                    PKind::Debris,
                    p - vec2(0.0, 14.0),
                    5,
                    110.0,
                    rgb(120, 100, 80),
                );
                self.fx.add(
                    p - vec2(0.0, 20.0),
                    0.9,
                    Kind::Text {
                        text: format!("+{bounty}"),
                        color: rgb(250, 210, 80),
                    },
                );
                if kind == sc::EnemyKind::Leader {
                    // The flag goes down with them.
                    self.fx
                        .burst(PKind::Debris, p - vec2(0.0, 40.0), 8, 90.0, rgb(170, 30, 30));
                    self.cleaning.send(self.game.map(), dist, false);
                    if playing {
                        self.banner = Some((
                            "SENT TO CLEANING".into(),
                            "The leader is suited up and marched to the airlock.".into(),
                            3.0,
                        ));
                    }
                }
            }
            Event::Explosion { pos, radius } => {
                let c = px(pos);
                let r = radius * TILE;
                self.fx.add(c, 0.5, Kind::Cloud { r });
                self.fx.burst(PKind::Ember, c, 8, 110.0, rgb(170, 255, 120));
                // A cloud of gas rolls out over the splash area and hangs there.
                for _ in 0..18 {
                    let a = rand::gen_range(0.0, std::f32::consts::TAU);
                    let v = vec2(a.cos(), a.sin() * 0.55) * rand::gen_range(0.3, 1.0) * r * 1.6;
                    let size = rand::gen_range(0.14, 0.26) * r;
                    let life = rand::gen_range(1.4, 2.6);
                    self.fx.gas(c, v, life, size, gas_color());
                }
            }
            Event::Shot {
                kind: TowerKind::Gas,
                tower,
                from,
                to,
            } => {
                // The tube coughs out a jet of gas as the canister leaves.
                let at = self
                    .game
                    .tower(tower)
                    .map_or(px(from), |t| tower_anchor(self.game.map(), t));
                let (tip, dir) = gas_muzzle(at, Some(px(to)));
                for _ in 0..9 {
                    let spread = vec2(-dir.y, dir.x) * rand::gen_range(-0.5, 0.5);
                    let v = (dir + spread) * rand::gen_range(30.0, 110.0);
                    let size = rand::gen_range(3.0, 6.0);
                    self.fx
                        .gas(tip, v, rand::gen_range(0.6, 1.3), size, gas_color());
                }
            }
            Event::Shot {
                kind: TowerKind::Deputy,
                from,
                to,
                ..
            } => {
                // Brass casing flicked out sideways.
                let p = px(from);
                let side = if to.x > from.x { -1.0 } else { 1.0 };
                let v = vec2(
                    side * rand::gen_range(40.0, 90.0),
                    rand::gen_range(-170.0, -110.0),
                );
                self.fx.particle(PKind::Casing, p, v, 0.7, WHITE);
            }
            Event::Pulse { pos, radius, .. } => self.fx.add(
                px(pos),
                0.45,
                Kind::Ring {
                    max_r: radius * TILE,
                    color: rgb(200, 130, 255),
                },
            ),
            Event::Shot {
                kind: TowerKind::Marksman,
                from,
                to,
                ..
            } => self.fx.add(px(from), 0.18, Kind::Tracer { to: px(to) }),
            Event::EnemyShot { from, to, .. } => {
                self.fx.add(px(from), 0.15, Kind::Tracer { to: px(to) });
                self.fx
                    .burst(PKind::Spark, px(to), 6, 160.0, rgb(255, 200, 120));
            }
            Event::TowerDestroyed { kind, pos, .. } => {
                let p = px(pos);
                self.fx.shake = self.fx.shake.max(0.2);
                self.fx.add(p, 0.6, Kind::Puff);
                self.fx.burst(PKind::Spark, p, 18, 260.0, rgb(255, 180, 90));
                self.fx.burst(PKind::Debris, p, 12, 180.0, rgb(90, 94, 100));
                self.fx.add(
                    p,
                    0.5,
                    Kind::Ring {
                        max_r: 40.0,
                        color: rgb(255, 150, 60),
                    },
                );
                self.fx.add(
                    p - vec2(0.0, 24.0),
                    1.4,
                    Kind::Text {
                        text: format!("{} DESTROYED", kind.name().to_uppercase()),
                        color: rgb(255, 120, 80),
                    },
                );
            }
            Event::ItCollapsed { seconds } if playing => {
                self.fx.shake = 0.5;
                self.banner = Some((
                    "IT HAS FALLEN".into(),
                    format!("The server room is down. No cameras below Up Top for {seconds:.0}s."),
                    3.5,
                ));
            }
            Event::ItRestored if playing && self.game.phase() == Phase::Wave => {
                self.banner = Some(("IT BACK ONLINE".into(), "Cameras restored.".into(), 1.8));
            }
            Event::PowerCut { .. } if playing && self.power_announced != self.game.wave() => {
                self.power_announced = self.game.wave();
                self.banner = Some((
                    "MAIN POWER CUT".into(),
                    "Mechanical is killing the lights. A whole section of the silo goes dark."
                        .into(),
                    3.0,
                ));
            }
            Event::SupplyJoined if playing => {
                self.fx.shake = 0.3;
                self.banner = Some((
                    "SUPPLY JOINS THE UPRISING".into(),
                    "Rebels passing the Supply depot now get guns and tools. \
                     Stop them below Supply, or barricade the stair above it."
                        .into(),
                    5.0,
                ));
            }
            Event::Equipped {
                pos, gun, tools, ..
            } => {
                let label = match (gun, tools) {
                    (true, true) => "+GUN +TOOLS",
                    (true, false) => "+GUN",
                    _ => "+TOOLS",
                };
                self.fx.add(
                    px(pos) - vec2(0.0, 30.0),
                    1.0,
                    Kind::Text {
                        text: label.into(),
                        color: rgb(240, 150, 80),
                    },
                );
            }
            Event::Arrested {
                kind, pos, cost, ..
            } => {
                let p = px(pos);
                let leader = kind == sc::EnemyKind::Leader;
                self.fx.add(
                    p - vec2(0.0, 44.0),
                    1.4,
                    Kind::Text {
                        text: format!("SENT TO CLEANING -{cost}"),
                        color: rgb(240, 210, 110),
                    },
                );
                if leader && playing {
                    self.banner = Some((
                        "CAPTURED FOR CLEANING".into(),
                        "Sims' agents take the flag-bearer alive.".into(),
                        3.0,
                    ));
                }
            }
            Event::BarrierUpgraded { id, level } => {
                if let Some(b) = self.game.barrier(id) {
                    let p = px(b.pos);
                    self.fx.add(p, 0.5, Kind::Ring { max_r: 30.0, color: rgb(250, 210, 80) });
                    let name = if level == 2 { "SANDBAG WALL" } else { "RIOT GATE" };
                    self.fx.add(
                        p - vec2(0.0, 40.0),
                        1.1,
                        Kind::Text { text: name.into(), color: rgb(250, 210, 80) },
                    );
                }
            }
            Event::BarriersRepaired { rebuilt } if playing => {
                if rebuilt > 0 {
                    self.fx.toast(format!(
                        "IT REBUILT {rebuilt} BARRIER{}",
                        if rebuilt == 1 { "" } else { "S" }
                    ));
                }
            }
            Event::BarrierBroken { pos, .. } => {
                let p = px(pos);
                self.fx
                    .burst(PKind::Debris, p, 16, 200.0, rgb(130, 100, 64));
                self.fx.shake = self.fx.shake.max(0.2);
                self.fx.add(p, 0.6, Kind::Puff);
                self.fx.add(
                    p - vec2(0.0, 26.0),
                    1.2,
                    Kind::Text {
                        text: "BARRIER BROKEN".into(),
                        color: rgb(255, 160, 90),
                    },
                );
            }
            Event::Lost => self.fx.shake = 0.8,
            _ => {}
        }
    }

    // ------------------------------------------------------------ commands

    fn report<T>(&mut self, r: Result<T, sc::CommandError>) {
        if let Err(e) = r {
            self.fx.toast(e.to_string().to_uppercase());
        }
    }

    fn send_wave(&mut self) {
        let r = self.game.send_wave();
        self.report(r);
    }

    fn upgrade_selected(&mut self) {
        if let Some(id) = self.selected {
            let r = self.game.upgrade(id);
            self.report(r);
        }
        if let Some(id) = self.selected_barrier {
            let r = self.game.upgrade_barrier(id);
            self.report(r);
        }
    }

    /// Pay Judicial to arrest the selected rebel.
    fn arrest_selected(&mut self) {
        let Some(id) = self.selected_enemy else { return };
        let Some(e) = self.game.enemies().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        match self.game.arrest(id) {
            Ok(_) => {
                self.judicial.arrest(e);
                self.selected_enemy = None;
            }
            Err(err) => self.report(Err::<(), _>(err)),
        }
    }

    /// Can `tool` go on tile (c, r) right now (ground, reach and free; cost aside)?
    fn spot_ok(&self, tool: Tool, c: i32, r: i32) -> bool {
        let Some(i) = tool_index(tool) else { return false };
        sc::map::in_bounds(c, r)
            && self.spots[i][r as usize][c as usize]
            && self.game.tower_at(c, r).is_none()
            && self.game.barrier_at(c, r).is_none()
    }

    /// Why `tool` can't go on tile (c, r): (short label, full hint). None if it can (or the
    /// only problem is money, which the normal error already explains).
    fn spot_problem(&self, tool: Tool, c: i32, r: i32) -> Option<(&'static str, String)> {
        if self.spot_ok(tool, c, r) || !sc::map::in_bounds(c, r) {
            return None;
        }
        if self.game.tower_at(c, r).is_some() || self.game.barrier_at(c, r).is_some() {
            return Some(("TAKEN", "SOMETHING IS ALREADY THERE".into()));
        }
        let map = self.game.map();
        let tile = map.tile(c, r);
        let where_ = ground_short(tool);
        Some(match tool {
            Tool::Tower(k) => {
                let name = k.name().to_uppercase();
                let goes = format!("{name} GOES ON {}", k.ground().to_uppercase());
                if map.tower_allowed(k, c, r) {
                    ("OUT OF REACH", "OUT OF REACH: REBELS NEVER PASS WITHIN RANGE HERE".into())
                } else if tile == sc::Tile::Shaft {
                    ("NOT IN MID-AIR", format!("NOT IN MID-AIR - {goes}"))
                } else if map.buildable(c, r) {
                    (where_, goes)
                } else {
                    ("CAN'T BUILD HERE", format!("CAN'T BUILD HERE - {goes}"))
                }
            }
            Tool::Barrier => {
                if tile == sc::Tile::Stairs {
                    ("ABOVE SUPPLY ONLY", "BARRIERS ONLY GO ABOVE THE SUPPLY DEPOT".into())
                } else {
                    ("STAIRS ONLY", "BARRIERS GO ON THE STAIRS, ABOVE SUPPLY".into())
                }
            }
            Tool::Arrest => return None,
        })
    }

    /// A tap that tried to build in the wrong place: say why and where it does go.
    fn wrong_spot(&mut self, hint: String) {
        self.fx.toast(hint);
    }

    /// The living, visible rebel whose body is under world pixel `p`.
    fn rebel_at(&self, p: Vec2) -> Option<sc::Id> {
        self.game
            .enemies()
            .iter()
            .filter(|e| e.hp > 0.0 && self.game.is_visible(e.pos))
            .map(|e| {
                let body = px(e.pos) + vec2(e.lane * TILE * 0.9, -22.0);
                (e.id, body.distance(p))
            })
            .filter(|&(_, d)| d < 22.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    fn sell_selected(&mut self) {
        if let Some(id) = self.selected {
            let r = self.game.sell(id);
            self.report(r);
            self.selected = None;
        }
        if let Some(id) = self.selected_barrier.take() {
            let r = self.game.remove_barrier(id);
            self.report(r);
        }
    }

    fn keys(&mut self) {
        if self.screen == Screen::Title {
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
                self.start();
            }
            return;
        }
        let kinds = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4];
        for (i, k) in kinds.iter().enumerate() {
            if is_key_pressed(*k) {
                self.pick(Tool::Tower(TowerKind::ALL[i]));
            }
        }
        if is_key_pressed(KeyCode::Key5) || is_key_pressed(KeyCode::B) {
            self.pick(Tool::Barrier);
        }
        if is_key_pressed(KeyCode::Escape) {
            self.build = None;
            self.selected = None;
            self.selected_barrier = None;
            self.selected_enemy = None;
        }
        if is_key_pressed(KeyCode::J) {
            if self.selected_enemy.is_some() {
                self.arrest_selected();
            } else {
                self.toggle_arrest_mode();
            }
        }
        if is_key_pressed(KeyCode::U) {
            self.upgrade_selected();
        }
        if is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::Space) {
            self.send_wave();
        }
        if is_key_pressed(KeyCode::F) {
            self.cycle_speed();
        }
        if is_key_pressed(KeyCode::P) {
            self.paused = !self.paused;
        }
        if is_key_pressed(KeyCode::T) {
            self.autopilot = !self.autopilot;
        }
        if is_key_pressed(KeyCode::M) {
            self.toggle_music();
        }
        if is_key_pressed(KeyCode::U) {
            self.upgrade_selected();
        }
        if is_key_pressed(KeyCode::X) || is_key_pressed(KeyCode::Backspace) {
            self.sell_selected();
        }
        if is_key_pressed(KeyCode::R) && self.game.is_over() {
            self.start();
        }
        for k in [KeyCode::Up, KeyCode::W, KeyCode::PageUp] {
            if is_key_pressed(k) {
                self.roll(-1);
            }
        }
        for k in [KeyCode::Down, KeyCode::S, KeyCode::PageDown] {
            if is_key_pressed(k) {
                self.roll(1);
            }
        }
        for (i, k) in [KeyCode::Home, KeyCode::End].iter().enumerate() {
            if is_key_pressed(*k) {
                self.set_view(i * (SECTION_COUNT - 1));
            }
        }
    }

    // ------------------------------------------------------------ view rolling

    fn set_view(&mut self, v: usize) {
        let v = v.min(SECTION_COUNT - 1);
        if v != self.view {
            self.view_label = 1.6;
        }
        self.view = v;
    }

    fn roll(&mut self, dir: i32) {
        let v = (self.view as i32 + dir).clamp(0, SECTION_COUNT as i32 - 1);
        self.set_view(v as usize);
    }

    fn update_scroll(&mut self, dt: f32) {
        self.view_label = (self.view_label - dt).max(0.0);
        if self.screen == Screen::Title {
            // Attract mode: drift slowly down the silo and back up.
            self.scroll = (1.0 - (self.time * 0.12).cos()) / 2.0 * max_scroll();
            return;
        }
        // Mouse wheel / trackpad: one section per gesture.
        self.wheel_cooldown = (self.wheel_cooldown - dt).max(0.0);
        let wy = if self.shot.is_none() {
            mouse_wheel().1
        } else {
            0.0
        };
        if self.wheel_cooldown > 0.0 {
            self.wheel = 0.0;
        } else {
            self.wheel += wy;
            if self.wheel.abs() >= 1.0 {
                self.roll(if self.wheel > 0.0 { -1 } else { 1 });
                self.wheel = 0.0;
                self.wheel_cooldown = 0.35;
            }
        }
        if self.drag.as_ref().is_some_and(|d| d.moved) {
            return; // the finger owns the scroll
        }
        let target = view_scroll(self.view);
        self.scroll += (target - self.scroll) * (1.0 - (-12.0 * dt).exp());
        if (target - self.scroll).abs() < 0.5 {
            self.scroll = target;
        }
    }

    /// Up/down arrows and the overview map on the right wall.
    fn navigator(&mut self, ui: &mut Ui) {
        let (up, map, down) = nav_rects();
        // Rebels out of sight above / below the field.
        let (top, bottom) = (self.scroll, self.scroll + FIELD_H);
        let wy = |e: &sc::Enemy| px(e.pos).y - HUD_H;
        let seen = || {
            self.game
                .enemies()
                .iter()
                .filter(|e| self.game.is_visible(e.pos))
        };
        let above = seen().filter(|e| wy(e) < top).count();
        let below = seen().filter(|e| wy(e) > bottom).count();
        arrow(up, true, self.view > 0, ui.hovered(up), above, self.time);
        arrow(
            down,
            false,
            self.view + 1 < SECTION_COUNT,
            ui.hovered(down),
            below,
            self.time,
        );
        minimap(&self.game, map, self.scroll, self.time);
        if ui.hit(up) {
            self.roll(-1);
        }
        if ui.hit(down) {
            self.roll(1);
        }
        if let Some(p) = ui.tap
            && ui.hit(map)
        {
            let row = ((p.y - map.y) / map.h * sc::map::ROWS as f32) as i32;
            self.set_view(sc::map::section_of_row(row));
        }
    }

    fn draw_view_label(&self) {
        if self.view_label <= 0.0 || self.screen != Screen::Playing {
            return;
        }
        let a = (self.view_label / 0.4).min(1.0);
        let (name, first, last) = SECTIONS[self.view];
        let label = format!(
            "{name}   LEVELS {}-{}",
            sc::map::level_of_row(first),
            sc::map::level_of_row(last) + 3
        );
        let w = measure(&label, 20.0) + 36.0;
        let y = HUD_H + 14.0;
        draw_rectangle(
            W / 2.0 - w / 2.0,
            y,
            w,
            34.0,
            rgba(10, 12, 16, (a * 210.0) as u8),
        );
        draw_rectangle(
            W / 2.0 - w / 2.0,
            y + 32.0,
            w,
            2.0,
            rgba(220, 184, 92, (a * 255.0) as u8),
        );
        text_centered(
            &label,
            W / 2.0,
            y + 24.0,
            20.0,
            rgba(240, 214, 130, (a * 255.0) as u8),
        );
    }

    fn pick(&mut self, k: Tool) {
        self.sound(Sfx::Click, None);
        self.selected = None;
        self.selected_barrier = None;
        self.selected_enemy = None;
        self.build = if self.build == Some(k) { None } else { Some(k) };
    }

    fn cycle_speed(&mut self) {
        self.speed = match self.speed {
            1 => 2,
            2 => 4,
            _ => 1,
        };
    }

    /// Presses in the field: drag vertically to roll the view, release without dragging to tap.
    fn field_input(&mut self, ui: &mut Ui) {
        if let Some(p) = ui.tap.filter(|_| !ui.consumed)
            && p.y >= HUD_H
            && p.y < PANEL_Y
        {
            self.drag = Some(Drag {
                start: p,
                scroll0: self.scroll,
                moved: false,
            });
        }
        let Some(drag) = &mut self.drag else { return };
        let dy = ui.pointer.y - drag.start.y;
        if dy.abs() > 14.0 {
            drag.moved = true;
        }
        if drag.moved {
            self.scroll = (drag.scroll0 - dy).clamp(0.0, max_scroll());
        }
        if is_mouse_button_down(MouseButton::Left) {
            return;
        }
        let drag = self.drag.take().unwrap();
        if !drag.moved {
            self.field_tap(drag.start + vec2(0.0, self.scroll));
            return;
        }
        // Snap: a decent flick rolls one section that way; otherwise go to the nearest.
        let from = (0..SECTION_COUNT)
            .min_by(|&a, &b| {
                (view_scroll(a) - drag.scroll0)
                    .abs()
                    .total_cmp(&(view_scroll(b) - drag.scroll0).abs())
            })
            .unwrap();
        let nearest = (0..SECTION_COUNT)
            .min_by(|&a, &b| {
                (view_scroll(a) - self.scroll)
                    .abs()
                    .total_cmp(&(view_scroll(b) - self.scroll).abs())
            })
            .unwrap();
        self.view = from;
        if nearest != from {
            self.set_view(nearest);
        } else if dy < -70.0 {
            self.roll(1);
        } else if dy > 70.0 {
            self.roll(-1);
        }
    }

    /// A tap on world pixel `p`.
    fn field_tap(&mut self, p: Vec2) {
        if self.game.is_over() {
            return;
        }
        // Arrest mode: the tapped rebel is arrested right away.
        if self.build == Some(Tool::Arrest) {
            match self.rebel_at(p) {
                Some(id) => {
                    // Arrest on the spot; if it fails (e.g. no chits) just say why and stay
                    // in Sims mode, without opening the rebel's panel over the tool cards.
                    self.selected_enemy = Some(id);
                    self.arrest_selected();
                    self.selected_enemy = None;
                }
                None => self.fx.toast("TAP A REBEL ON THE STAIR"),
            }
            return;
        }
        // Rebels first: tapping one offers Judicial's arrest.
        if let Some(id) = self.rebel_at(p) {
            self.selected_enemy = Some(id);
            self.selected = None;
            self.selected_barrier = None;
            self.build = None;
            return;
        }
        self.selected_enemy = None;
        let Some((c, r)) = tile_at(p) else { return };
        if let Some(b) = self.game.barrier_at(c, r) {
            self.selected_barrier = Some(b.id);
            self.selected = None;
            self.build = None;
        } else if let Some(t) = self.game.tower_at(c, r) {
            self.selected = Some(t.id);
            self.selected_barrier = None;
            self.build = None;
        } else if let Some(Tool::Tower(k)) = self.build {
            if let Some((_, hint)) = self.spot_problem(Tool::Tower(k), c, r) {
                self.wrong_spot(hint);
            } else {
                let res = self.game.place(k, c, r);
                self.report(res);
            }
        } else if self.build == Some(Tool::Barrier) {
            if let Some((_, hint)) = self.spot_problem(Tool::Barrier, c, r) {
                self.wrong_spot(hint);
            } else {
                let res = self.game.place_barrier(c, r);
                self.report(res);
            }
        } else {
            self.selected = None;
            self.selected_barrier = None;
        }
    }

    // ------------------------------------------------------------ drawing

    fn field_overlays(&self, ui: &Ui, world_pointer: Vec2) {
        if self.screen != Screen::Playing {
            return;
        }
        if let Some(t) = self.selected.and_then(|id| self.game.tower(id)) {
            range_area(px(t.pos), t.reach * TILE, true);
            let c = tower_anchor(self.game.map(), t);
            draw_circle_lines(c.x, c.y, 25.0, 2.5, rgb(255, 255, 255));
        }
        if let Some(b) = self.selected_barrier.and_then(|id| self.game.barrier(id)) {
            let c = px(b.pos);
            draw_rectangle_lines(c.x - 25.0, c.y - 34.0, 50.0, 44.0, 2.5, WHITE);
        }
        let aim = if self.build == Some(Tool::Arrest) {
            self.rebel_at(world_pointer)
        } else {
            None
        };
        if let Some(e) = self
            .selected_enemy
            .or(aim)
            .and_then(|id| self.game.enemies().iter().find(|e| e.id == id))
        {
            // Judicial's crosshair on the rebel.
            let c = px(e.pos) + vec2(e.lane * TILE * 0.9, -22.0);
            let pulse = 1.0 + 0.08 * (self.time * 6.0).sin();
            let r = 20.0 * pulse;
            let col = rgb(255, 214, 90);
            draw_circle_lines(c.x, c.y, r, 2.0, col);
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let d = vec2(dx, dy);
                let (a, b) = (c + d * (r - 5.0), c + d * (r + 5.0));
                draw_line(a.x, a.y, b.x, b.y, 2.0, col);
            }
        }
        let (up, map, down) = nav_rects();
        let over_nav = [up, map, down].iter().any(|r| r.contains(ui.pointer));
        let hovering =
            ui.pointer.y >= HUD_H && ui.pointer.y < PANEL_Y && !over_nav && self.drag.is_none();
        if let Some(tool) = self.build.filter(|t| *t != Tool::Arrest) {
            // Hovering a wrong tile (desktop): a red cross and why.
            if let Some((c, r)) = tile_at(world_pointer).filter(|_| hovering)
                && let Some((short, _)) = self.spot_problem(tool, c, r)
            {
                let (x, y) = (c as f32 * TILE, HUD_H + r as f32 * TILE);
                let red = rgba(255, 90, 80, 210);
                draw_line(x + 12.0, y + 10.0, x + TILE - 12.0, y + TILE - 14.0, 2.5, red);
                draw_line(x + TILE - 12.0, y + 10.0, x + 12.0, y + TILE - 14.0, 2.5, red);
                let w = measure(short, 12.0) + 10.0;
                let lx = (x + TILE / 2.0 - w / 2.0).clamp(4.0, W - w - 4.0);
                draw_rectangle(lx, y + TILE - 14.0, w, 16.0, rgba(40, 8, 8, 220));
                text(short, lx + 5.0, y + TILE - 2.0, 12.0, rgb(255, 170, 160));
            }
        }
        if self.build == Some(Tool::Barrier) {
            // Show where barriers may go: the stair above the Supply depot.
            let y = HUD_H + sc::map::SUPPLY_ROOM[0].1 as f32 * TILE;
            let mut x = TILE;
            while x < W - TILE {
                draw_line(x, y, x + 14.0, y, 3.0, rgba(255, 120, 80, 200));
                x += 24.0;
            }
            text(
                "NO BARRIERS BELOW SUPPLY",
                TILE + 8.0,
                y + 18.0,
                13.0,
                rgb(255, 150, 110),
            );
            text(
                "BARRIERS ALLOWED ABOVE",
                TILE + 8.0,
                y - 8.0,
                13.0,
                rgb(140, 230, 150),
            );
            if let Some((c, r)) = tile_at(world_pointer).filter(|_| hovering) {
                let ok = self.game.can_place_barrier(c, r).is_ok();
                match self.game.map().barrier_dist(c, r) {
                    Some(d) => {
                        let p = px(self.game.map().pos_at(d));
                        barrier_icon(p, 1.0, 1, 1.0, false, 0.6);
                        let col = if ok {
                            rgb(120, 230, 140)
                        } else {
                            rgb(255, 90, 80)
                        };
                        draw_rectangle_lines(p.x - 24.0, p.y - 30.0, 48.0, 40.0, 2.0, col);
                    }
                    None => draw_rectangle_lines(
                        c as f32 * TILE + 2.0,
                        HUD_H + r as f32 * TILE + 2.0,
                        TILE - 4.0,
                        TILE - 4.0,
                        2.0,
                        rgba(255, 90, 80, 160),
                    ),
                }
            }
            return;
        }
        // Build ghost under the pointer (desktop hover).
        let build = match self.build {
            Some(Tool::Tower(k)) => Some(k),
            _ => None,
        };
        if let (Some(k), Some((c, r))) = (build, tile_at(world_pointer))
            && ui.pointer.y >= HUD_H
            && ui.pointer.y < PANEL_Y
            && !over_nav
            && self.drag.is_none()
            && self.game.tower_at(c, r).is_none()
        {
            {
                let ok = self.game.can_place(k, c, r).is_ok();
                let map = self.game.map();
                if map.tower_allowed(k, c, r) {
                    let pos = map.tower_pos(c, r);
                    let reach = sc::map::reach(pos, k.stats(1).range);
                    range_area(px(pos), reach * TILE, ok);
                    tower_icon(k, tile_anchor(map, c, r), 1, None, 0.0, 0.55);
                } else {
                    draw_rectangle_lines(
                        c as f32 * TILE + 2.0,
                        HUD_H + r as f32 * TILE + 2.0,
                        TILE - 4.0,
                        TILE - 4.0,
                        2.0,
                        rgba(255, 90, 80, 160),
                    );
                }
            }
        }
    }

    fn draw_banner(&self) {
        let Some((title, body, t)) = &self.banner else {
            return;
        };
        let a = (t / 0.4).min(1.0);
        let lines = wrap(body, 18.0, W - 80.0);
        let h = 50.0 + lines.len() as f32 * 22.0;
        let y = HUD_H + TILE * 1.2;
        draw_rectangle(20.0, y, W - 40.0, h, rgba(10, 12, 16, (a * 200.0) as u8));
        draw_rectangle(
            20.0,
            y,
            W - 40.0,
            3.0,
            rgba(220, 184, 92, (a * 255.0) as u8),
        );
        text_centered(
            title,
            W / 2.0,
            y + 32.0,
            28.0,
            rgba(240, 210, 120, (a * 255.0) as u8),
        );
        for (i, l) in lines.iter().enumerate() {
            text_centered(
                l,
                W / 2.0,
                y + 56.0 + i as f32 * 22.0,
                18.0,
                rgba(220, 220, 220, (a * 255.0) as u8),
            );
        }
    }

    fn hud(&mut self, ui: &mut Ui) {
        let g = &self.game;
        bar_background(Rect::new(0.0, 0.0, W, HUD_H), HUD_H);
        text(
            &format!("WAVE {}/{}", g.wave(), MAX_WAVES),
            14.0,
            32.0,
            26.0,
            WHITE,
        );
        let phase = match g.phase() {
            Phase::Building => ("BUILDING".to_string(), rgb(120, 220, 120)),
            Phase::Wave => (
                format!("{} CLIMBING", g.enemies().len() + g.pending_spawns()),
                rgb(240, 190, 80),
            ),
            Phase::Won => ("VICTORY".to_string(), rgb(120, 220, 120)),
            Phase::Lost => ("BREACHED".to_string(), rgb(255, 90, 70)),
        };
        text(&phase.0, 14.0, 56.0, 16.0, phase.1);

        // Chits.
        draw_circle(196.0, 36.0, 12.0, rgb(214, 170, 60));
        draw_circle_lines(196.0, 36.0, 12.0, 2.0, rgb(120, 90, 20));
        text_centered("C", 196.0, 42.0, 16.0, rgb(90, 60, 10));
        text(
            &format!("{}", g.chits()),
            216.0,
            44.0,
            28.0,
            rgb(250, 220, 120),
        );

        // Seals.
        text("SEALS", 312.0, 28.0, 14.0, rgb(160, 160, 160));
        let f = g.seals() as f32 / START_SEALS as f32;
        draw_rectangle(312.0, 36.0, 140.0, 14.0, rgb(40, 20, 20));
        let c = if f > 0.5 {
            rgb(90, 220, 110)
        } else if f > 0.25 {
            rgb(240, 200, 60)
        } else {
            rgb(255, 70, 50)
        };
        draw_rectangle(312.0, 36.0, 140.0 * f, 14.0, c);
        text(&format!("{}", g.seals()), 458.0, 49.0, 18.0, WHITE);
        // IT server room.
        if g.cameras_down() {
            let blink = (self.time * 6.0).sin() > 0.0;
            let c = if blink {
                rgb(255, 90, 70)
            } else {
                rgb(150, 50, 40)
            };
            text(
                &format!("IT DOWN {:.0}s", g.it_down().ceil()),
                312.0,
                66.0,
                13.0,
                c,
            );
        } else {
            text("IT", 312.0, 66.0, 13.0, rgb(110, 170, 240));
            let f = g.it_hp() / sc::stats::IT_HP;
            draw_rectangle(332.0, 58.0, 120.0, 7.0, rgb(20, 26, 40));
            draw_rectangle(332.0, 58.0, 120.0 * f, 7.0, rgb(90, 160, 255));
        }

        // Speed / pause / autopilot.
        let b = |i: f32| Rect::new(W - 190.0 + i * 47.0, 10.0, 43.0, 52.0);
        let speed = format!("{}x", self.speed);
        if ui.button(b(0.0), &[&speed], true, rgb(120, 160, 220)) {
            self.cycle_speed();
        }
        let pause = if self.paused { "GO" } else { "II" };
        if ui.button(b(1.0), &[pause], true, rgb(200, 200, 200)) {
            self.paused = !self.paused;
        }
        let ai_col = if self.autopilot {
            rgb(90, 230, 230)
        } else {
            rgb(110, 120, 130)
        };
        if ui.button(b(2.0), &["AI"], true, ai_col) {
            self.autopilot = !self.autopilot;
        }
        // Music on/off: a little speaker icon.
        let muted = self.audio_mode == 2 || (self.music.is_none() && self.sfx.is_none());
        let fx_only = self.audio_mode == 1;
        let r = b(3.0);
        let mus_col = if muted {
            rgb(110, 120, 130)
        } else {
            rgb(230, 190, 90)
        };
        if ui.button(
            r,
            &[""],
            self.music.is_some() || self.sfx.is_some(),
            mus_col,
        ) {
            self.toggle_music();
        }
        if fx_only {
            text_centered(
                "FX",
                r.x + r.w / 2.0,
                r.y + r.h - 4.0,
                10.0,
                rgb(230, 190, 90),
            );
        }
        let c = vec2(r.x + r.w / 2.0 - 4.0, r.y + r.h / 2.0);
        let ic = if muted { rgb(120, 120, 126) } else { WHITE };
        draw_rectangle(c.x - 6.0, c.y - 4.0, 5.0, 8.0, ic);
        draw_triangle(
            vec2(c.x - 2.0, c.y - 4.0),
            vec2(c.x + 5.0, c.y - 10.0),
            vec2(c.x + 5.0, c.y + 10.0),
            ic,
        );
        draw_triangle(
            vec2(c.x - 2.0, c.y - 4.0),
            vec2(c.x - 2.0, c.y + 4.0),
            vec2(c.x + 5.0, c.y + 10.0),
            ic,
        );
        if muted {
            draw_line(
                c.x + 8.0,
                c.y - 5.0,
                c.x + 16.0,
                c.y + 5.0,
                2.0,
                rgb(230, 90, 70),
            );
            draw_line(
                c.x + 8.0,
                c.y + 5.0,
                c.x + 16.0,
                c.y - 5.0,
                2.0,
                rgb(230, 90, 70),
            );
        } else {
            for k in 0..2 {
                let rr = 6.0 + k as f32 * 5.0;
                for i in 0..5 {
                    let a0 = -0.7 + i as f32 * 0.28;
                    let a1 = a0 + 0.28;
                    draw_line(
                        c.x + 4.0 + a0.cos() * rr,
                        c.y + a0.sin() * rr,
                        c.x + 4.0 + a1.cos() * rr,
                        c.y + a1.sin() * rr,
                        1.5,
                        ic,
                    );
                }
            }
        }
    }

    fn panel(&mut self, ui: &mut Ui) {
        let y = PANEL_Y;
        bar_background(Rect::new(0.0, y, W, PANEL_H), y);

        if let Some(t) = self.selected.and_then(|id| self.game.tower(id)).cloned() {
            let s = t.stats();
            let col = tower_color(t.kind);
            tower_icon(t.kind, vec2(48.0, y + 56.0), t.level, None, 0.0, 1.0);
            text(t.kind.name(), 88.0, y + 42.0, 26.0, col);
            text(
                &format!(
                    "LEVEL {}/{}   KILLS {}   HP {:.0}/{:.0}",
                    t.level, MAX_TOWER_LEVEL, t.kills, t.hp, t.max_hp
                ),
                88.0,
                y + 66.0,
                16.0,
                rgb(200, 200, 200),
            );
            let mut lines = vec![format!(
                "DMG {:.0}   REACH {:.1}   EVERY {:.2}s",
                s.damage, t.reach, s.cooldown
            )];
            if s.slow > 0.0 {
                lines.push(format!(
                    "SLOW {:.0}% FOR {:.1}s",
                    s.slow * 100.0,
                    s.slow_time
                ));
            }
            if s.splash > 0.0 {
                lines.push(format!("SPLASH {:.2}", s.splash));
            }
            if s.pierce {
                lines.push("IGNORES ARMOR".into());
            }
            for (i, l) in lines.iter().enumerate() {
                text(
                    l,
                    20.0,
                    y + 104.0 + i as f32 * 20.0,
                    16.0,
                    rgb(170, 170, 176),
                );
            }
            let up = t.upgrade_cost();
            let up_label = up.map_or("MAX LEVEL".to_string(), |c| format!("UPGRADE {c}"));
            let can_up = up.is_some_and(|c| c <= self.game.chits()) && !self.game.is_over();
            if ui.button(
                Rect::new(372.0, y + 14.0, 160.0, 70.0),
                &[&up_label],
                can_up,
                rgb(110, 200, 120),
            ) {
                self.upgrade_selected();
            }
            let sell = format!("SELL {}", t.sell_value());
            if ui.button(
                Rect::new(372.0, y + 92.0, 160.0, 70.0),
                &[&sell],
                !self.game.is_over(),
                rgb(220, 120, 90),
            ) {
                self.sell_selected();
            }
            if ui.button(
                Rect::new(540.0, y + 14.0, 124.0, 148.0),
                &["BACK"],
                true,
                rgb(150, 150, 160),
            ) {
                self.selected = None;
            }
            ui.hit(Rect::new(0.0, y, W, PANEL_H)); // swallow taps on the panel background
            return;
        }

        if let Some(b) = self
            .selected_barrier
            .and_then(|id| self.game.barrier(id))
            .cloned()
        {
            barrier_icon(vec2(48.0, y + 66.0), 1.3, b.level, b.hp / b.max_hp, b.broken, 1.0);
            let name = match b.level {
                1 => "BARRICADE",
                2 => "SANDBAG WALL",
                _ => "RIOT GATE",
            };
            text(name, 88.0, y + 42.0, 24.0, rgb(200, 160, 110));
            let status = if b.broken {
                format!("WRECKED   LEVEL {}/{}", b.level, BARRIER_MAX_LEVEL)
            } else {
                format!("HP {:.0}/{:.0}   LEVEL {}/{}", b.hp, b.max_hp, b.level, BARRIER_MAX_LEVEL)
            };
            text(&status, 88.0, y + 66.0, 16.0, rgb(200, 200, 200));
            let info = if b.broken {
                "Smashed. It no longer holds anyone back; IT rebuilds it once this wave is over."
            } else {
                "Rebels must break it to climb on. Bare hands take ages; tools from Supply \
                 smash it in seconds. Repaired after every wave."
            };
            for (i, l) in wrap(info, 15.0, 330.0).iter().enumerate() {
                text(l, 20.0, y + 104.0 + i as f32 * 20.0, 15.0, rgb(170, 170, 176));
            }
            let up = b.upgrade_cost();
            let up_label = if b.broken {
                "REBUILDING".to_string()
            } else {
                up.map_or("MAX LEVEL".to_string(), |c| format!("UPGRADE {c}"))
            };
            let can_up = !b.broken
                && up.is_some_and(|c| c <= self.game.chits())
                && !self.game.is_over();
            if ui.button(
                Rect::new(372.0, y + 14.0, 160.0, 70.0),
                &[&up_label],
                can_up,
                rgb(110, 200, 120),
            ) {
                self.upgrade_selected();
            }
            let label = format!("REMOVE +{}", b.refund());
            if ui.button(
                Rect::new(372.0, y + 92.0, 160.0, 70.0),
                &[&label],
                !self.game.is_over(),
                rgb(220, 120, 90),
            ) {
                self.sell_selected();
            }
            if ui.button(
                Rect::new(540.0, y + 14.0, 124.0, 148.0),
                &["BACK"],
                true,
                rgb(150, 150, 160),
            ) {
                self.selected_barrier = None;
            }
            ui.hit(Rect::new(0.0, y, W, PANEL_H));
            return;
        }

        if let Some(e) = self
            .selected_enemy
            .and_then(|id| self.game.enemies().iter().find(|e| e.id == id))
            .cloned()
        {
            let leader = e.kind == sc::EnemyKind::Leader;
            // Draw the rebel in the panel, standing on a small plinth.
            let foot = vec2(48.0, y + 96.0);
            draw_ellipse(foot.x, foot.y, 24.0, 5.0, 0.0, rgb(40, 42, 48));
            let mut shown = e.clone();
            // `rebel` stands the figure 4 px above px(pos): place pos so its feet land here.
            shown.pos = sc::vec2(foot.x / TILE, (foot.y + 4.0 - HUD_H) / TILE);
            shown.lane = 0.0;
            shown.blocked = true;
            render::rebel(&shown, true, self.time);
            text(e.kind.name(), 88.0, y + 42.0, 24.0, rgb(240, 170, 120));
            let mut tags = vec![format!("HP {:.0}/{:.0}", e.hp, e.max_hp)];
            if e.armed {
                tags.push("ARMED".into());
            }
            if e.tools {
                tags.push("TOOLS".into());
            }
            text(&tags.join("   "), 88.0, y + 66.0, 16.0, rgb(200, 200, 200));
            let info = if leader {
                "Carries the flag. Sims' agents take them alive at three times the price, \
                 and march them to cleaning."
            } else {
                "Pay Judicial: one of Sims' agents arrests them on the stair and marches \
                 them to cleaning. No bounty for arrests."
            };
            for (i, l) in wrap(info, 15.0, 330.0).iter().enumerate() {
                text(l, 20.0, y + 112.0 + i as f32 * 20.0, 15.0, rgb(170, 170, 176));
            }
            let cost = self.game.arrest_cost(&e);
            let can = cost <= self.game.chits() && !self.game.is_over();
            let lines: Vec<String> = if leader {
                vec!["CAPTURE".into(), "FOR CLEANING".into(), format!("{cost}")]
            } else {
                vec!["ARREST".into(), "FOR CLEANING".into(), format!("{cost}")]
            };
            let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
            if ui.button(
                Rect::new(372.0, y + 14.0, 160.0, 148.0),
                &refs,
                can,
                rgb(240, 200, 90),
            ) {
                self.arrest_selected();
            }
            if ui.button(
                Rect::new(540.0, y + 14.0, 124.0, 148.0),
                &["BACK"],
                true,
                rgb(150, 150, 160),
            ) {
                self.selected_enemy = None;
            }
            ui.hit(Rect::new(0.0, y, W, PANEL_H));
            return;
        }

        for (i, &k) in TowerKind::ALL.iter().enumerate() {
            let r = Rect::new(8.0 + i as f32 * 110.0, y + 12.0, 104.0, 152.0);
            let sel = self.build == Some(Tool::Tower(k));
            let afford = self.game.chits() >= k.cost();
            let col = tower_color(k);
            let fill = if sel {
                shade(col, 0.35)
            } else if ui.hovered(r) {
                rgb(36, 38, 44)
            } else {
                rgb(28, 30, 34)
            };
            panel(r, fill, if sel { col } else { rgb(60, 62, 68) });
            tower_icon(
                k,
                vec2(r.x + r.w / 2.0, r.y + 44.0),
                1,
                None,
                0.0,
                if afford { 1.0 } else { 0.4 },
            );
            text_centered(
                k.name(),
                r.x + r.w / 2.0,
                r.y + 96.0,
                15.0,
                if afford { WHITE } else { rgb(120, 120, 120) },
            );
            let cost_col = if afford {
                rgb(250, 210, 90)
            } else {
                rgb(200, 80, 70)
            };
            text_centered(
                &format!("{}", k.cost()),
                r.x + r.w / 2.0,
                r.y + 122.0,
                22.0,
                cost_col,
            );
            text_centered(
                ground_short(Tool::Tower(k)),
                r.x + r.w / 2.0,
                r.y + 142.0,
                10.0,
                rgb(140, 140, 146),
            );
            text(
                &format!("{}", i + 1),
                r.x + 8.0,
                r.y + 20.0,
                14.0,
                rgb(110, 110, 116),
            );
            if ui.hit(r) {
                self.pick(Tool::Tower(k));
            }
        }
        // Barrier card.
        {
            let r = Rect::new(8.0 + 4.0 * 110.0, y + 12.0, 104.0, 152.0);
            let sel = self.build == Some(Tool::Barrier);
            let afford = self.game.chits() >= BARRIER_COST;
            let col = rgb(200, 160, 110);
            let fill = if sel {
                shade(col, 0.35)
            } else if ui.hovered(r) {
                rgb(36, 38, 44)
            } else {
                rgb(28, 30, 34)
            };
            panel(r, fill, if sel { col } else { rgb(60, 62, 68) });
            barrier_icon(
                vec2(r.x + r.w / 2.0, r.y + 58.0),
                1.2,
                1,
                1.0,
                false,
                if afford { 1.0 } else { 0.4 },
            );
            text_centered(
                "Barrier",
                r.x + r.w / 2.0,
                r.y + 96.0,
                15.0,
                if afford { WHITE } else { rgb(120, 120, 120) },
            );
            let cost_col = if afford {
                rgb(250, 210, 90)
            } else {
                rgb(200, 80, 70)
            };
            text_centered(
                &format!("{BARRIER_COST}"),
                r.x + r.w / 2.0,
                r.y + 122.0,
                22.0,
                cost_col,
            );
            text("5", r.x + 8.0, r.y + 20.0, 14.0, rgb(110, 110, 116));
            text_centered(
                ground_short(Tool::Barrier),
                r.x + r.w / 2.0,
                r.y + 142.0,
                10.0,
                rgb(140, 140, 146),
            );
            if ui.hit(r) {
                self.pick(Tool::Barrier);
            }
        }
        let r = Rect::new(560.0, y + 12.0, 104.0, 152.0);
        let building = self.game.phase() == Phase::Building;
        if self.game.phase() == Phase::Wave {
            // During a wave this slot calls Sims' agents.
            self.sims_card(ui, r);
            ui.hit(Rect::new(0.0, y, W, PANEL_H));
            return;
        }
        let next = format!("WAVE {}", self.game.wave() + 1);
        let lines: Vec<&str> = if building {
            vec!["SEND", &next]
        } else {
            vec!["HOLD", "THE", "LINE"]
        };
        if ui.button(r, &lines, building, rgb(230, 170, 60)) {
            self.send_wave();
        }
        ui.hit(Rect::new(0.0, y, W, PANEL_H));
    }

    /// Are rebels close to the airlock (in Up Top, where the cameras can see them)?
    fn rebels_near_airlock(&self) -> bool {
        let up_top_last = SECTIONS[0].2 as f32;
        self.game
            .enemies()
            .iter()
            .any(|e| e.pos.y <= up_top_last && self.game.is_visible(e.pos))
    }

    /// The Judicial card: arm arrest mode. Pulses when rebels near the airlock.
    fn sims_card(&mut self, ui: &mut Ui, r: Rect) {
        let sel = self.build == Some(Tool::Arrest);
        let base = sc::stats::arrest_cost(sc::EnemyKind::Porter, self.game.wave());
        let afford = self.game.chits() >= base;
        let accent = rgb(240, 200, 90);
        let urgent = self.rebels_near_airlock();
        let pulse = if urgent && !sel { 0.5 + 0.5 * (self.time * 6.0).sin() } else { 0.0 };
        let fill = if sel {
            shade(accent, 0.4)
        } else if ui.hovered(r) {
            rgb(36, 38, 44)
        } else {
            rgb(28, 30, 34)
        };
        let border = if sel {
            accent
        } else if urgent {
            Color::new(1.0, 0.85 * pulse + 0.15, 0.3, 1.0)
        } else {
            rgb(60, 62, 68)
        };
        panel(r, fill, border);
        if urgent && !sel {
            draw_rectangle_lines(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, 2.0, with_alpha(accent, pulse));
        }
        crate::judicial::agent_icon(vec2(r.x + r.w / 2.0 - 6.0, r.y + 80.0), 2.2, if afford { 1.0 } else { 0.45 });
        let fg = if afford { WHITE } else { rgb(120, 120, 120) };
        text_centered("SIMS'", r.x + r.w / 2.0, r.y + 98.0, 15.0, fg);
        text_centered("AGENTS", r.x + r.w / 2.0, r.y + 114.0, 15.0, fg);
        let cost_col = if afford { rgb(250, 210, 80) } else { rgb(255, 120, 90) };
        text_centered(&format!("ARREST {base}+"), r.x + r.w / 2.0, r.y + 136.0, 14.0, cost_col);
        text("J", r.x + 8.0, r.y + 20.0, 14.0, rgb(110, 110, 116));
        if ui.hit(r) {
            self.toggle_arrest_mode();
        }
    }

    fn toggle_arrest_mode(&mut self) {
        if self.game.phase() != Phase::Wave {
            return;
        }
        self.sound(Sfx::Click, None);
        self.selected = None;
        self.selected_barrier = None;
        self.selected_enemy = None;
        if self.build == Some(Tool::Arrest) {
            self.build = None;
        } else {
            self.build = Some(Tool::Arrest);
            self.fx.toast("TAP A REBEL: SIMS' AGENTS ARREST THEM");
        }
    }

    fn title(&mut self, ui: &mut Ui) {
        draw_rectangle(0.0, 0.0, W, H, rgba(6, 6, 8, 170));
        let y = 260.0;
        text_shadow("SILO", W / 2.0, y, 120.0, rgb(230, 200, 120));
        text_shadow("LOCKDOWN", W / 2.0, y + 60.0, 44.0, rgb(200, 200, 200));
        let body = "You are IT. The uprising climbs the spiral stair from Mechanical, \
                    level by level. If they reach the airlock, the silo is opened and \
                    everyone will see the outside. Hold the Pact.";
        for (i, l) in wrap(body, 20.0, W - 120.0).iter().enumerate() {
            text_centered(
                l,
                W / 2.0,
                y + 130.0 + i as f32 * 28.0,
                20.0,
                rgb(210, 210, 210),
            );
        }
        let pulse = 0.6 + 0.4 * (self.time * 3.0).sin().abs();
        let r = Rect::new(W / 2.0 - 150.0, y + 300.0, 300.0, 76.0);
        if ui.button(
            r,
            &["BEGIN SHIFT"],
            true,
            with_alpha(rgb(230, 170, 60), pulse),
        ) || ui.hit(Rect::new(0.0, 0.0, W, H))
        {
            self.start();
        }
        text_centered(
            "1-4 build  5 barrier  N wave  U upgrade  X sell  W/S roll  F speed  T AI  M sound",
            W / 2.0,
            H - 40.0,
            15.0,
            rgb(140, 140, 140),
        );
    }

    fn game_over(&mut self, ui: &mut Ui) {
        let won = self.game.phase() == Phase::Won;
        let r = Rect::new(60.0, HUD_H + 220.0, W - 120.0, 300.0);
        panel(
            r,
            rgba(10, 12, 16, 235),
            if won {
                rgb(110, 220, 120)
            } else {
                rgb(255, 80, 60)
            },
        );
        let (title, body) = if won {
            (
                "THE SILO HOLDS",
                "The Pact is preserved. The outside remains a secret.",
            )
        } else {
            (
                "THE AIRLOCK IS OPEN",
                "The rebels step outside. IT has failed the silo.",
            )
        };
        text_shadow(
            title,
            W / 2.0,
            r.y + 60.0,
            36.0,
            if won {
                rgb(140, 240, 150)
            } else {
                rgb(255, 110, 90)
            },
        );
        for (i, l) in wrap(body, 20.0, r.w - 40.0).iter().enumerate() {
            text_centered(
                l,
                W / 2.0,
                r.y + 104.0 + i as f32 * 26.0,
                20.0,
                rgb(220, 220, 220),
            );
        }
        let t = self.game.totals();
        let stats = format!(
            "WAVE {}/{}    KILLS {}    BREACHES {}",
            self.game.wave(),
            MAX_WAVES,
            t.kills,
            t.breaches
        );
        text_centered(&stats, W / 2.0, r.y + 180.0, 18.0, rgb(180, 180, 180));
        if ui.button(
            Rect::new(W / 2.0 - 120.0, r.y + 210.0, 240.0, 66.0),
            &["NEW SHIFT"],
            true,
            rgb(230, 170, 60),
        ) {
            self.start();
        }
    }
}

/// Toxic green, varied a little per puff.
fn gas_color() -> Color {
    let k = rand::gen_range(0.85, 1.1);
    Color::new(0.45 * k, 0.82 * k, 0.34 * k, 0.8)
}

fn gas_color_faint() -> Color {
    let k = rand::gen_range(0.9, 1.1);
    Color::new(0.55 * k, 0.8 * k, 0.45 * k, 0.45)
}

/// Which tiles each tool may ever use (by ground type and reach), computed once: index
/// [Deputy, Jammer, Gas, Marksman, Barrier][row][col].
fn placement_spots(map: &sc::Map) -> Vec<Vec<Vec<bool>>> {
    use sc::map::{COLS, ROWS};
    let grid = |f: &dyn Fn(i32, i32) -> bool| -> Vec<Vec<bool>> {
        (0..ROWS).map(|r| (0..COLS).map(|c| f(c, r)).collect()).collect()
    };
    let mut v: Vec<Vec<Vec<bool>>> = TowerKind::ALL
        .iter()
        .map(|&k| grid(&|c, r| map.tower_allowed(k, c, r) && map.in_reach(c, r, k.stats(1).range)))
        .collect();
    v.push(grid(&|c, r| map.barrier_allowed(c, r)));
    v
}

fn tool_index(tool: Tool) -> Option<usize> {
    match tool {
        Tool::Tower(k) => TowerKind::ALL.iter().position(|&x| x == k),
        Tool::Barrier => Some(4),
        Tool::Arrest => None,
    }
}

/// Short "where it goes" label for build cards and hover hints.
fn ground_short(tool: Tool) -> &'static str {
    match tool {
        Tool::Tower(TowerKind::Deputy) => "LEVELS + STAIRS",
        Tool::Tower(TowerKind::Gas) => "STAIRS ONLY",
        Tool::Tower(_) => "LEVELS ONLY",
        Tool::Barrier => "STAIRS ABOVE SUPPLY",
        Tool::Arrest => "",
    }
}
