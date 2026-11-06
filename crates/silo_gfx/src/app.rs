//! Game loop glue: input, fixed-step simulation, events -> effects, HUD and command panel.

use crate::fx::{Fx, Kind};
use crate::render::{self, *};
use crate::ui::*;
use macroquad::prelude::*;
use silo_core as sc;
use silo_core::bot::Bot;
use silo_core::stats::{MAX_TOWER_LEVEL, MAX_WAVES, START_SEALS};
use silo_core::{Event, Game, Phase, TICK, TowerKind};

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
    build: Option<TowerKind>,
    selected: Option<sc::Id>,
    speed: u32,
    paused: bool,
    acc: f32,
    time: f32,
    fx: Fx,
    banner: Option<(String, String, f32)>,
    decor: Decor,
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
            speed: 2,
            paused: false,
            acc: 0.0,
            time: 0.0,
            fx: Fx::default(),
            banner: None,
            decor,
            shot,
            frame: 0,
            quit: false,
        };
        if !title {
            app.start();
            app.autopilot = true;
            app.speed = 4;
        }
        app
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    fn start(&mut self) {
        self.game = Game::new(seed());
        self.bot = Bot::balanced(self.game.map());
        self.screen = Screen::Playing;
        self.autopilot = false;
        self.build = Some(TowerKind::Deputy);
        self.selected = None;
        self.speed = 1;
        self.paused = false;
        self.acc = 0.0;
        self.fx.clear();
        self.banner = Some((
            "YOU ARE IT".into(),
            "Mechanical is rising. Build on the levels and catwalks, then send the wave.".into(),
            5.0,
        ));
    }

    pub fn frame(&mut self) {
        let dt = get_frame_time().min(0.1);
        self.time += dt;
        self.frame += 1;

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
        let tap = is_mouse_button_pressed(MouseButton::Left).then_some(pointer);
        let mut ui = Ui {
            pointer,
            tap,
            consumed: false,
        };

        self.keys();
        self.simulate(dt);
        self.fx.update(dt);
        if let Some((_, _, t)) = &mut self.banner {
            *t -= dt;
            if *t <= 0.0 {
                self.banner = None;
            }
        }

        clear_background(rgb(8, 8, 10));
        set_camera(&cam);
        render::world(&self.game, &self.decor, self.time, self.fx.red_flash);
        self.field_overlays(&ui);
        self.fx.draw();
        vignette();
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
                if self.game.is_over() {
                    self.game_over(&mut ui);
                }
                self.field_tap(&mut ui);
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

    fn simulate(&mut self, dt: f32) {
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
        for ev in self.game.drain_events() {
            self.on_event(ev);
        }
    }

    fn on_event(&mut self, ev: Event) {
        let playing = self.screen == Screen::Playing;
        let airlock = vec2(6.0 * TILE, HUD_H + TILE * 0.5);
        match ev {
            Event::WaveStarted { wave, lore } if playing => {
                self.banner = Some((format!("WAVE {wave}"), lore.to_string(), 4.0));
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
                kind, pos, bounty, ..
            } => {
                let p = px(pos);
                self.fx.add(p - vec2(0.0, 10.0), 0.5, Kind::Puff);
                self.fx.add(
                    p - vec2(0.0, 20.0),
                    0.9,
                    Kind::Text {
                        text: format!("+{bounty}"),
                        color: rgb(250, 210, 80),
                    },
                );
                if kind == sc::EnemyKind::Leader && playing {
                    self.banner = Some((
                        "LEADER DOWN".into(),
                        "The uprising falters... for now.".into(),
                        2.5,
                    ));
                }
            }
            Event::Explosion { pos, radius } => {
                self.fx.add(px(pos), 0.8, Kind::Cloud { r: radius * TILE })
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
    }

    fn sell_selected(&mut self) {
        if let Some(id) = self.selected {
            let r = self.game.sell(id);
            self.report(r);
            self.selected = None;
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
                self.pick(TowerKind::ALL[i]);
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            self.build = None;
            self.selected = None;
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
        if is_key_pressed(KeyCode::U) {
            self.upgrade_selected();
        }
        if is_key_pressed(KeyCode::X) || is_key_pressed(KeyCode::Backspace) {
            self.sell_selected();
        }
        if is_key_pressed(KeyCode::R) && self.game.is_over() {
            self.start();
        }
    }

    fn pick(&mut self, k: TowerKind) {
        self.selected = None;
        self.build = if self.build == Some(k) { None } else { Some(k) };
    }

    fn cycle_speed(&mut self) {
        self.speed = match self.speed {
            1 => 2,
            2 => 4,
            _ => 1,
        };
    }

    fn field_tap(&mut self, ui: &mut Ui) {
        let Some(p) = ui.tap.filter(|_| !ui.consumed) else {
            return;
        };
        if p.y < HUD_H || p.y >= PANEL_Y || self.game.is_over() {
            return;
        }
        let Some((c, r)) = tile_at(p) else { return };
        if let Some(t) = self.game.tower_at(c, r) {
            self.selected = Some(t.id);
            self.build = None;
        } else if let Some(k) = self.build {
            let res = self.game.place(k, c, r);
            self.report(res);
        } else {
            self.selected = None;
        }
    }

    // ------------------------------------------------------------ drawing

    fn field_overlays(&self, ui: &Ui) {
        if self.screen != Screen::Playing {
            return;
        }
        if let Some(t) = self.selected.and_then(|id| self.game.tower(id)) {
            let c = px(t.pos);
            range_circle(c, t.stats().range * TILE, true);
            draw_circle_lines(c.x, c.y, 25.0, 2.5, rgb(255, 255, 255));
        }
        // Build ghost under the pointer (desktop hover).
        if let (Some(k), Some((c, r))) = (self.build, tile_at(ui.pointer))
            && ui.pointer.y >= HUD_H
            && ui.pointer.y < PANEL_Y
            && self.game.tower_at(c, r).is_none()
        {
            {
                let ok = self.game.can_place(k, c, r).is_ok();
                let center = px(sc::map::tile_center(c, r));
                if self.game.map().buildable(c, r) {
                    range_circle(center, k.stats(1).range * TILE, ok);
                    tower_icon(k, center, 1, None, 0.0, 0.55);
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
        draw_rectangle(0.0, 0.0, W, HUD_H, rgb(16, 18, 22));
        draw_line(0.0, HUD_H, W, HUD_H, 2.0, rgb(70, 64, 50));
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

        // Speed / pause / autopilot.
        let b = |i: f32| Rect::new(W - 178.0 + i * 58.0, 10.0, 52.0, 52.0);
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
    }

    fn panel(&mut self, ui: &mut Ui) {
        let y = PANEL_Y;
        draw_rectangle(0.0, y, W, PANEL_H, rgb(20, 22, 26));
        draw_line(0.0, y, W, y, 2.0, rgb(70, 64, 50));

        if let Some(t) = self.selected.and_then(|id| self.game.tower(id)).cloned() {
            let s = t.stats();
            let col = tower_color(t.kind);
            tower_icon(t.kind, vec2(48.0, y + 56.0), t.level, None, 0.0, 1.0);
            text(t.kind.name(), 88.0, y + 42.0, 26.0, col);
            text(
                &format!("LEVEL {}/{}   KILLS {}", t.level, MAX_TOWER_LEVEL, t.kills),
                88.0,
                y + 66.0,
                16.0,
                rgb(200, 200, 200),
            );
            let mut lines = vec![format!(
                "DMG {:.0}   RANGE {:.1}   EVERY {:.2}s",
                s.damage, s.range, s.cooldown
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

        for (i, &k) in TowerKind::ALL.iter().enumerate() {
            let r = Rect::new(8.0 + i as f32 * 134.0, y + 12.0, 126.0, 152.0);
            let sel = self.build == Some(k);
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
                17.0,
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
            text(
                &format!("{}", i + 1),
                r.x + 8.0,
                r.y + 20.0,
                14.0,
                rgb(110, 110, 116),
            );
            if ui.hit(r) {
                self.pick(k);
            }
        }
        let r = Rect::new(546.0, y + 12.0, 118.0, 152.0);
        let building = self.game.phase() == Phase::Building;
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
            "1-4 build   N wave   U upgrade   X sell   F speed   T autopilot",
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
