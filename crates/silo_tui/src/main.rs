//! Terminal frontend for Silo: Lockdown. Thin layer over `silo_core`: all rules live in the core.

use ratatui::DefaultTerminal;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyCode, KeyEventKind,
    MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};
use silo_core::bot::Bot;
use silo_core::map::{COLS, PLAQUES, ROWS};
use silo_core::stats::{MAX_TOWER_LEVEL, MAX_WAVES, START_SEALS};
use silo_core::*;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Each tile is drawn as CW x CH terminal cells.
const CW: i32 = 4;
const CH: i32 = 2;
const FIELD_W: u16 = (COLS * CW) as u16;
const FIELD_H: u16 = (ROWS * CH) as u16;
const FRAME: Duration = Duration::from_micros(33_333);

fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let result = App::new().run(&mut terminal);
    execute!(std::io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}

struct Effect {
    cells: Vec<(i32, i32)>,
    ch: char,
    color: Color,
    ttl: u8,
}

struct App {
    game: Game,
    bot: Bot,
    autopilot: bool,
    cursor: (i32, i32),
    build: Option<TowerKind>,
    speed: u32,
    paused: bool,
    effects: Vec<Effect>,
    log: VecDeque<Line<'static>>,
    airlock_flash: u8,
    field_rect: Rect,
    quit: bool,
}

impl App {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        let game = Game::new(seed);
        let bot = Bot::balanced(game.map());
        let mut app = Self {
            game,
            bot,
            autopilot: false,
            cursor: (2, 11),
            build: Some(TowerKind::Deputy),
            speed: 1,
            paused: false,
            effects: Vec::new(),
            log: VecDeque::new(),
            airlock_flash: 0,
            field_rect: Rect::default(),
            quit: false,
        };
        app.say(Line::from("You are IT. Mechanical is rising.".bold()));
        app.say(Line::from("Build on the walls, then press N to send the wave."));
        app
    }

    fn run(&mut self, term: &mut DefaultTerminal) -> std::io::Result<()> {
        let mut next = Instant::now();
        while !self.quit {
            term.draw(|f| self.draw(f))?;
            // Handle input until the next frame is due.
            loop {
                let now = Instant::now();
                if now >= next {
                    break;
                }
                if event::poll(next - now)? {
                    self.handle(event::read()?);
                }
            }
            next += FRAME;
            if Instant::now() > next + FRAME * 10 {
                next = Instant::now(); // we fell far behind (e.g. suspended); don't spiral
            }
            self.update();
        }
        Ok(())
    }

    // ------------------------------------------------------------ logic

    fn update(&mut self) {
        if self.autopilot && self.game.phase() == Phase::Building {
            self.bot.take_turn(&mut self.game);
        }
        if !self.paused {
            for _ in 0..self.speed {
                self.game.step();
                if self.autopilot && self.game.phase() == Phase::Wave {
                    self.bot.spend(&mut self.game);
                }
            }
        }
        for ev in self.game.drain_events() {
            self.on_event(ev);
        }
        self.effects.retain_mut(|e| {
            e.ttl = e.ttl.saturating_sub(1);
            e.ttl > 0
        });
        self.airlock_flash = self.airlock_flash.saturating_sub(1);
    }

    fn on_event(&mut self, ev: Event) {
        match ev {
            Event::WaveStarted { wave, lore } => {
                self.say(Line::from(vec![
                    format!("WAVE {wave}: ").yellow().bold(),
                    Span::raw(lore).italic(),
                ]));
            }
            Event::WaveCleared { wave, bonus } => {
                self.say(Line::from(format!("Wave {wave} held. +{bonus} chits.")).green())
            }
            Event::Breach { kind, seals_lost, .. } => {
                self.airlock_flash = 8;
                self.say(Line::from(format!("{} reached the airlock! -{seals_lost} seal", kind.name())).red());
            }
            Event::EnemyKilled { kind, pos, bounty, .. } => {
                if kind == EnemyKind::Leader {
                    self.say(Line::from(format!("The rebel leader has fallen. +{bounty}")).magenta().bold());
                }
                self.effect(vec![to_cell(pos)], 'x', Color::DarkGray, 8);
            }
            Event::Explosion { pos, radius } => {
                self.effect(cells_in_circle(pos, radius), '░', Color::LightGreen, 5);
            }
            Event::Pulse { pos, radius, .. } => {
                let ring = cells_in_circle(pos, radius)
                    .into_iter()
                    .filter(|&c| (cell_center(c).distance(pos) - radius).abs() < 0.3)
                    .collect();
                self.effect(ring, '·', Color::Magenta, 4);
            }
            Event::Shot { kind: TowerKind::Marksman, from, to, .. } => {
                let n = (from.distance(to) * 8.0).ceil() as i32;
                let line = (1..n).map(|i| to_cell(from.lerp(to, i as f32 / n as f32))).collect();
                self.effect(line, '∙', Color::LightRed, 3);
            }
            Event::Won => self.say(Line::from("THE SILO HOLDS. The Pact endures.").green().bold()),
            Event::Lost => self.say(Line::from("THE AIRLOCK IS OPEN. They see the outside.").red().bold()),
            _ => {}
        }
    }

    fn effect(&mut self, cells: Vec<(i32, i32)>, ch: char, color: Color, ttl: u8) {
        self.effects.push(Effect { cells, ch, color, ttl });
    }

    fn say(&mut self, line: Line<'static>) {
        self.log.push_front(line);
        self.log.truncate(40);
    }

    fn report(&mut self, r: Result<(), CommandError>) {
        if let Err(e) = r {
            self.say(Line::from(format!("✗ {e}")).red());
        }
    }

    fn act_at_cursor(&mut self) {
        let (c, r) = self.cursor;
        if self.game.tower_at(c, r).is_some() {
            self.upgrade_at_cursor();
        } else if let Some(kind) = self.build {
            let res = self.game.place(kind, c, r).map(|_| ());
            self.report(res);
        } else {
            self.say(Line::from("Pick a tower with 1-4 first.").dark_gray());
        }
    }

    fn upgrade_at_cursor(&mut self) {
        let res = match self.game.tower_at(self.cursor.0, self.cursor.1) {
            Some(t) => self.game.upgrade(t.id).map(|_| ()),
            None => Err(CommandError::NoSuchTower),
        };
        self.report(res);
    }

    fn sell_at_cursor(&mut self) {
        let res = match self.game.tower_at(self.cursor.0, self.cursor.1) {
            Some(t) => self.game.sell(t.id).map(|v| self.say(Line::from(format!("Sold for {v}.")))),
            None => Err(CommandError::NoSuchTower),
        };
        self.report(res);
    }

    fn handle(&mut self, ev: TermEvent) {
        match ev {
            TermEvent::Key(k) if k.kind != KeyEventKind::Release => {
                let (c, r) = self.cursor;
                match k.code {
                    KeyCode::Char('q') => self.quit = true,
                    KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('a') => self.cursor.0 = (c - 1).max(0),
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('d') => {
                        self.cursor.0 = (c + 1).min(COLS - 1)
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => self.cursor.1 = (r - 1).max(0),
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => {
                        self.cursor.1 = (r + 1).min(ROWS - 1)
                    }
                    KeyCode::Char(ch @ '1'..='4') => {
                        let kind = TowerKind::ALL[(ch as u8 - b'1') as usize];
                        self.build = if self.build == Some(kind) { None } else { Some(kind) };
                    }
                    KeyCode::Esc => self.build = None,
                    KeyCode::Enter | KeyCode::Char(' ') => self.act_at_cursor(),
                    KeyCode::Char('u') => self.upgrade_at_cursor(),
                    KeyCode::Char('x') | KeyCode::Backspace | KeyCode::Delete => self.sell_at_cursor(),
                    KeyCode::Char('n') => {
                        let res = self.game.send_wave().map(|_| ());
                        self.report(res);
                    }
                    KeyCode::Char('f') => self.speed = if self.speed >= 4 { 1 } else { self.speed * 2 },
                    KeyCode::Char('p') => self.paused = !self.paused,
                    KeyCode::Char('t') => {
                        self.autopilot = !self.autopilot;
                        let s = if self.autopilot { "Autopilot ON (IT AI plays)." } else { "Autopilot OFF." };
                        self.say(Line::from(s).cyan());
                    }
                    KeyCode::Char('r') if self.game.is_over() => *self = App::new(),
                    _ => {}
                }
            }
            TermEvent::Mouse(m) => {
                let f = self.field_rect;
                let (x, y) = (m.column, m.row);
                if x < f.x || y < f.y || x >= f.x + f.width || y >= f.y + f.height {
                    return;
                }
                let tile = (((x - f.x) as i32) / CW, ((y - f.y) as i32) / CH);
                match m.kind {
                    MouseEventKind::Moved => self.cursor = tile,
                    MouseEventKind::Down(MouseButton::Left) => {
                        self.cursor = tile;
                        self.act_at_cursor();
                    }
                    MouseEventKind::Down(MouseButton::Right) => {
                        self.cursor = tile;
                        self.sell_at_cursor();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ drawing

    fn draw(&mut self, f: &mut ratatui::Frame) {
        let area = f.area();
        let need = (FIELD_W + 2 + 40, FIELD_H + 3);
        if area.width < need.0 || area.height < need.1 {
            let msg = format!("Terminal too small: need {}x{}, have {}x{}", need.0, need.1, area.width, area.height);
            f.render_widget(Paragraph::new(msg).red(), area);
            return;
        }
        let [main, help] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let [left, right] =
            Layout::horizontal([Constraint::Length(FIELD_W + 2), Constraint::Min(38)]).areas(main);
        let [field_box, _] =
            Layout::vertical([Constraint::Length(FIELD_H + 2), Constraint::Min(0)]).areas(left);

        let title = Line::from(" THE STAIRWELL ".bold()).centered();
        let block = Block::bordered().title(title).border_style(Style::new().dark_gray());
        let inner = block.inner(field_box);
        self.field_rect = inner;
        f.render_widget(block, field_box);
        f.render_widget(FieldView { app: self }, inner);

        if self.game.is_over() {
            self.draw_game_over(f, inner);
        }
        self.draw_panel(f, right);

        let help_line = Line::from(
            [
                ("←↑↓→", "move"),
                ("1-4", "pick"),
                ("⏎", "build/upg"),
                ("u", "upgrade"),
                ("x", "sell"),
                ("n", "wave"),
                ("f", "speed"),
                ("p", "pause"),
                ("t", "auto"),
                ("q", "quit"),
            ]
            .into_iter()
            .flat_map(|(k, v)| [format!(" {k} ").black().on_gray(), format!(" {v} ").into()])
            .collect::<Vec<Span>>(),
        );
        f.render_widget(Paragraph::new(help_line), help);
    }

    fn draw_game_over(&self, f: &mut ratatui::Frame, field: Rect) {
        let won = self.game.phase() == Phase::Won;
        let w = 40;
        let h = 9;
        let r = Rect::new(field.x + (field.width - w) / 2, field.y + (field.height - h) / 2, w, h);
        let t = self.game.totals();
        let (title, color, text) = if won {
            (" THE SILO HOLDS ", Color::Green, "The Pact is preserved.\nThe outside stays a secret.")
        } else {
            (" THE AIRLOCK IS OPEN ", Color::Red, "The rebels step outside.\nIT has failed the silo.")
        };
        let body = format!(
            "{text}\n\nWave {}/{MAX_WAVES}  Kills {}  Breaches {}\n\n[r] restart   [q] quit",
            self.game.wave(),
            t.kills,
            t.breaches
        );
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(body).centered().block(
                Block::bordered()
                    .title(Line::from(title).bold().centered())
                    .border_style(Style::new().fg(color)),
            ),
            r,
        );
    }

    fn draw_panel(&self, f: &mut ratatui::Frame, area: Rect) {
        let g = &self.game;
        let [status, shop, info, log] = Layout::vertical([
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Min(3),
        ])
        .areas(area);

        // Status
        let phase = match g.phase() {
            Phase::Building => "BUILDING".green(),
            Phase::Wave => format!("WAVE IN PROGRESS ({} queued, {} climbing)", g.pending_spawns(), g.enemies().len())
                .yellow(),
            Phase::Won => "VICTORY".green().bold(),
            Phase::Lost => "BREACHED".red().bold(),
        };
        let seals_bar: String = (0..START_SEALS).map(|i| if i < g.seals() { '█' } else { '░' }).collect();
        let seal_color = match g.seals() {
            s if s > START_SEALS / 2 => Color::Green,
            s if s > START_SEALS / 4 => Color::Yellow,
            _ => Color::Red,
        };
        let status_lines = vec![
            Line::from(vec!["Wave  ".dark_gray(), format!("{}/{MAX_WAVES}  ", g.wave()).bold(), phase]),
            Line::from(vec!["Chits ".dark_gray(), format!("{}", g.chits()).yellow().bold()]),
            Line::from(vec![
                "Seals ".dark_gray(),
                Span::styled(seals_bar, Style::new().fg(seal_color)),
                format!(" {}", g.seals()).into(),
            ]),
            Line::from(vec![
                "Speed ".dark_gray(),
                format!("{}x", self.speed).bold(),
                if self.paused { "  PAUSED".red().bold() } else { "".into() },
                if self.autopilot { "  AUTOPILOT".cyan().bold() } else { "".into() },
                format!("  t={:.0}s", g.time()).dark_gray(),
            ]),
            if g.phase() == Phase::Building {
                Line::from("Press N to send the next wave.".green().italic())
            } else {
                Line::default()
            },
        ];
        f.render_widget(
            Paragraph::new(status_lines).block(Block::bordered().title(" SILO: LOCKDOWN ".bold())),
            status,
        );

        // Shop
        let shop_lines: Vec<Line> = TowerKind::ALL
            .iter()
            .enumerate()
            .map(|(i, &k)| {
                let selected = self.build == Some(k);
                let affordable = g.chits() >= k.cost();
                let style = if selected {
                    Style::new().fg(Color::Black).bg(tower_color(k))
                } else if affordable {
                    Style::new().fg(tower_color(k))
                } else {
                    Style::new().fg(Color::DarkGray)
                };
                Line::from(vec![
                    Span::styled(format!(" {} {} {:<12} {:>4} ", i + 1, tower_glyph(k), k.name(), k.cost()), style),
                    format!(" {}", k.blurb()).dark_gray(),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(shop_lines).block(Block::bordered().title(" ARMORY ")), shop);

        // Cursor info
        let (c, r) = self.cursor;
        let mut lines = vec![];
        if let Some(t) = g.tower_at(c, r) {
            let s = t.stats();
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", t.kind.name()), Style::new().fg(tower_color(t.kind)).bold()),
                format!("Lv {}/{MAX_TOWER_LEVEL}", t.level).into(),
                format!("   kills {}", t.kills).dark_gray(),
            ]));
            lines.push(Line::from(format!(
                "dmg {:.0}  range {:.1}  every {:.2}s{}",
                s.damage,
                s.range,
                s.cooldown,
                if s.pierce { "  pierce" } else { "" }
            )));
            if s.slow > 0.0 {
                lines.push(Line::from(format!("slow {:.0}% for {:.1}s", s.slow * 100.0, s.slow_time)));
            }
            if s.splash > 0.0 {
                lines.push(Line::from(format!("splash {:.2}", s.splash)));
            }
            lines.push(match t.upgrade_cost() {
                Some(cost) => {
                    let n = t.kind.stats(t.level + 1);
                    Line::from(vec![
                        "[u] upgrade ".into(),
                        format!("{cost}").yellow(),
                        format!(" → dmg {:.0} range {:.1}", n.damage, n.range).dark_gray(),
                    ])
                }
                None => Line::from("max level".dark_gray()),
            });
            lines.push(Line::from(vec!["[x] sell for ".into(), format!("{}", t.sell_value()).yellow()]));
        } else {
            let plaque = PLAQUES.iter().find(|p| p.0 == r || p.0 == r + 1);
            let where_ = plaque.map_or(String::new(), |p| format!(" (level ~{}, {})", p.1, p.2));
            let desc = match g.map().tile(c, r) {
                Tile::Floor => format!("Living quarters{where_}. Buildable."),
                Tile::Shaft => format!("Catwalk over the stairwell{where_}. Buildable."),
                Tile::Stairs => "The spiral stair. The uprising climbs here.".into(),
                Tile::Rock => "Silo wall. Solid concrete and earth.".into(),
                Tile::Airlock => "The airlock. Do not let it open.".into(),
                Tile::Outside => "The outside. Toxic. Nobody goes there.".into(),
                Tile::OffMap => String::new(),
            };
            lines.push(Line::from(desc));
            if let (Some(k), true) = (self.build, g.map().buildable(c, r)) {
                lines.push(match g.can_place(k, c, r) {
                    Ok(()) => Line::from(format!("[⏎] build {} for {}", k.name(), k.cost())).green(),
                    Err(e) => Line::from(format!("{e}")).red(),
                });
            }
        }
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(format!(" TILE {c},{r} "))),
            info,
        );

        // Log
        let log_lines: Vec<Line> = self.log.iter().take(log.height.saturating_sub(2) as usize).cloned().collect();
        f.render_widget(
            Paragraph::new(log_lines).wrap(Wrap { trim: true }).block(Block::bordered().title(" RADIO ")),
            log,
        );
    }
}

// ---------------------------------------------------------------- field widget

struct FieldView<'a> {
    app: &'a App,
}

fn tower_color(k: TowerKind) -> Color {
    match k {
        TowerKind::Deputy => Color::Rgb(90, 160, 255),
        TowerKind::Jammer => Color::Rgb(200, 110, 255),
        TowerKind::Gas => Color::Rgb(120, 220, 110),
        TowerKind::Marksman => Color::Rgb(255, 100, 90),
    }
}

fn tower_glyph(k: TowerKind) -> char {
    match k {
        TowerKind::Deputy => 'D',
        TowerKind::Jammer => 'J',
        TowerKind::Gas => 'G',
        TowerKind::Marksman => 'M',
    }
}

fn enemy_glyph(k: EnemyKind) -> char {
    match k {
        EnemyKind::Porter => 'p',
        EnemyKind::Mechanic => 'm',
        EnemyKind::Miner => 'M',
        EnemyKind::Leader => '&',
    }
}

fn to_cell(p: Vec2) -> (i32, i32) {
    ((p.x * CW as f32).floor() as i32, (p.y * CH as f32).floor() as i32)
}

fn cell_center((x, y): (i32, i32)) -> Vec2 {
    vec2((x as f32 + 0.5) / CW as f32, (y as f32 + 0.5) / CH as f32)
}

fn cells_in_circle(c: Vec2, r: f32) -> Vec<(i32, i32)> {
    let (x0, y0) = to_cell(vec2(c.x - r, c.y - r));
    let (x1, y1) = to_cell(vec2(c.x + r, c.y + r));
    (x0..=x1)
        .flat_map(|x| (y0..=y1).map(move |y| (x, y)))
        .filter(|&cell| cell_center(cell).distance(c) <= r)
        .collect()
}

/// Cell at field-relative (x, y), or None if outside the field.
fn cell_at(buf: &mut Buffer, area: Rect, x: i32, y: i32) -> Option<&mut ratatui::buffer::Cell> {
    if x < 0 || y < 0 || x >= area.width as i32 || y >= area.height as i32 {
        return None;
    }
    buf.cell_mut((area.x + x as u16, area.y + y as u16))
}

const HAZE_BG: Color = Color::Rgb(58, 52, 44);
const DIRT_BG: Color = Color::Rgb(66, 54, 40);
const ROCK_BG: Color = Color::Rgb(44, 37, 32);
const FLOOR_BG: Color = Color::Rgb(36, 38, 42);
const SHAFT_BG: Color = Color::Rgb(14, 16, 22);
const PILLAR_FG: Color = Color::Rgb(78, 82, 90);
const STAIR_FRONT: Color = Color::Rgb(200, 138, 72);
const STAIR_BACK: Color = Color::Rgb(92, 68, 50);

/// Cheap deterministic per-cell noise for textures.
fn noise(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 13;
    h.wrapping_mul(0xC2B2_AE3D) >> 24
}

/// Draw one pass of the spiral stair. Half-block glyphs give 2 sub-rows per cell, so the
/// diagonal reads as a run of steps.
fn draw_stair(buf: &mut Buffer, area: Rect, a: Vec2, b: Vec2, fg: Color) {
    if (a.x - b.x).abs() < 1e-3 {
        // Vertical hatch ladder up into the airlock.
        let x = (a.x * CW as f32).floor() as i32;
        let (y0, y1) = ((b.y * CH as f32) as i32, (a.y * CH as f32) as i32);
        for y in y0..=y1 {
            for (dx, ch) in [(-1, '╟'), (0, '╢')] {
                if let Some(c) = cell_at(buf, area, x + dx, y) {
                    c.set_char(ch).set_fg(fg);
                }
            }
        }
        return;
    }
    let (x0, x1) = ((a.x.min(b.x) * CW as f32).round() as i32, (a.x.max(b.x) * CW as f32).round() as i32);
    for x in x0..x1 {
        let wx = (x as f32 + 0.5) / CW as f32;
        let t = (wx - a.x) / (b.x - a.x);
        let wy = (a.y + (b.y - a.y) * t) * CH as f32;
        let y = wy.floor() as i32;
        let ch = if wy - (y as f32) < 0.5 { '▀' } else { '▄' };
        if let Some(c) = cell_at(buf, area, x, y) {
            c.set_char(ch).set_fg(fg);
        }
    }
    // Landings at the turns.
    for p in [a, b] {
        let (px, py) = to_cell(p);
        for dx in -1..=0 {
            if let Some(c) = cell_at(buf, area, px + dx, py) {
                c.set_char('▬').set_fg(fg);
            }
        }
    }
}

fn dim(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r / 5 * 3, g / 5 * 3, b / 5 * 3),
        other => other,
    }
}

fn section_short(name: &str) -> String {
    name.split_whitespace().last().unwrap_or("").chars().take(4).collect()
}

impl Widget for FieldView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let app = self.app;
        let g = &app.game;
        let map = g.map();
        let put = |buf: &mut Buffer, cx: i32, cy: i32, ch: char, style: Style| {
            if let Some(cell) = cell_at(buf, area, cx, cy) {
                cell.set_char(ch).set_style(style);
            }
        };
        let lost = g.phase() == Phase::Lost;
        let silo_x = (CW, (COLS - 1) * CW); // inner wall span in cells

        // Background, per cell.
        for y in 0..FIELD_H as i32 {
            for x in 0..FIELD_W as i32 {
                let (c, r) = (x / CW, y / CH);
                let sub = y % CH;
                let n = noise(x, y);
                let (ch, style) = match map.tile(c, r) {
                    Tile::Outside | Tile::Airlock if sub == 0 => {
                        // The outside: grey-brown haze, dead hills.
                        let ch = if n < 40 { '░' } else if n < 46 { '·' } else { ' ' };
                        (ch, Style::new().bg(HAZE_BG).fg(Color::Rgb(84, 76, 62)))
                    }
                    Tile::Outside | Tile::Airlock => {
                        if (silo_x.0..silo_x.1).contains(&x) {
                            ('▄', Style::new().bg(HAZE_BG).fg(Color::Rgb(110, 110, 104))) // silo roof
                        } else {
                            ('▓', Style::new().bg(DIRT_BG).fg(Color::Rgb(84, 68, 50)))
                        }
                    }
                    Tile::Rock => {
                        let ch = if n < 60 { '▒' } else if n < 90 { '░' } else { ' ' };
                        (ch, Style::new().bg(ROCK_BG).fg(Color::Rgb(62, 52, 44)))
                    }
                    Tile::Floor => {
                        if sub == CH - 1 {
                            ('▁', Style::new().bg(FLOOR_BG).fg(Color::Rgb(96, 98, 104))) // floor slab
                        } else if n < 6 {
                            ('•', Style::new().bg(FLOOR_BG).fg(Color::Rgb(150, 130, 70))) // a bulb
                        } else {
                            (' ', Style::new().bg(FLOOR_BG))
                        }
                    }
                    Tile::Stairs | Tile::Shaft => (' ', Style::new().bg(SHAFT_BG)),
                    Tile::OffMap => continue,
                };
                put(buf, x, y, ch, style);
            }
        }

        // Concrete inner face of the cylinder wall, curving in at the dome.
        let concrete = Style::new().bg(ROCK_BG).fg(Color::Rgb(120, 122, 126));
        for y in CH..FIELD_H as i32 {
            let dome = y < 2 * CH; // row 1: the shoulders
            let (l, r) = if dome { (2 * CW - 1, (COLS - 2) * CW) } else { (CW - 1, (COLS - 1) * CW) };
            put(buf, l, y, '▐', concrete);
            put(buf, r, y, '▌', concrete);
        }
        for x in CW..2 * CW {
            put(buf, x, CH, '▄', concrete);
            put(buf, (COLS - 1) * CW - 1 - (x - CW), CH, '▄', concrete);
        }

        // Level numbers on the left wall, section names on the right.
        let label = Style::new().bg(ROCK_BG).fg(Color::Rgb(214, 180, 90)).bold();
        for &(row, level, name) in PLAQUES {
            for (dx, ch) in format!("{level:>3}").chars().enumerate() {
                put(buf, dx as i32, row * CH, ch, label);
            }
            for (dx, ch) in format!("{:>4}", section_short(name)).chars().enumerate() {
                put(buf, (COLS - 1) * CW + dx as i32, row * CH, ch, label);
            }
        }

        // The spiral: back passes, then the central pillar, then front passes on top.
        for (a, b, front) in map.segments() {
            if !front {
                draw_stair(buf, area, a, b, STAIR_BACK);
            }
        }
        let px = (map::PILLAR_X * CW as f32) as i32;
        for y in CH..FIELD_H as i32 {
            for (dx, ch) in [(-1, '▐'), (0, '▌')] {
                if let Some(c) = cell_at(buf, area, px + dx, y).filter(|c| c.symbol() == " ") {
                    c.set_char(ch).set_fg(PILLAR_FG);
                }
            }
        }
        for (a, b, front) in map.segments() {
            if front {
                draw_stair(buf, area, a, b, STAIR_FRONT);
            }
        }

        // The airlock door.
        let danger = 1.0 - g.seals() as f32 / START_SEALS as f32;
        let red = (110.0 + 145.0 * danger) as u8;
        let frame_fg = if app.airlock_flash % 2 == 1 { Color::Red } else { Color::Rgb(red, 90, 80) };
        let x0 = map::AIRLOCK[0].0 * CW;
        let (top, bottom) = if lost { ("▛▀    ▀▜", "▌      ▐") } else { ("▛▀▀▀▀▀▀▜", "▌◖████◗▐") };
        for (dy, row) in [top, bottom].iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                let bg = if lost && (2..6).contains(&dx) { Color::Rgb(235, 230, 210) } else { Color::Rgb(40, 40, 44) };
                let fg = if ch == '█' || ch == '◖' || ch == '◗' { Color::Rgb(150, 154, 160) } else { frame_fg };
                put(buf, x0 + dx as i32, dy as i32, ch, Style::new().bg(bg).fg(fg).bold());
            }
        }

        // Range preview for the cursor (selected tower, or build candidate).
        let (cc, cr) = app.cursor;
        let preview = g
            .tower_at(cc, cr)
            .map(|t| t.stats().range)
            .or_else(|| app.build.filter(|_| map.buildable(cc, cr)).map(|k| k.stats(1).range));
        if let Some(range) = preview {
            for cell in cells_in_circle(map::tile_center(cc, cr), range) {
                if let Some(c) = cell_at(buf, area, cell.0, cell.1) {
                    let bg = match c.bg {
                        Color::Rgb(r, gg, b) => Color::Rgb(r.saturating_add(18), gg.saturating_add(30), b.saturating_add(18)),
                        other => other,
                    };
                    c.set_bg(bg);
                }
            }
        }

        // Towers
        for t in g.towers() {
            let (x0, y0) = (t.col * CW, t.row * CH);
            let col = tower_color(t.kind);
            let ready = t.cooldown <= 0.0;
            let top = Style::new().bg(Color::Rgb(20, 22, 26)).fg(col).bold();
            let label = format!("[{}{}]", tower_glyph(t.kind), if ready { '·' } else { '•' });
            for (dx, ch) in label.chars().enumerate() {
                put(buf, x0 + dx as i32, y0, ch, top);
            }
            let pips: String = (0..MAX_TOWER_LEVEL).map(|i| if i < t.level { '▪' } else { ' ' }).collect();
            for (dx, ch) in format!(" {pips}").chars().enumerate() {
                put(buf, x0 + dx as i32, y0 + 1, ch, Style::new().bg(Color::Rgb(20, 22, 26)).fg(Color::Yellow));
            }
        }

        // Effects (below enemies)
        for e in &app.effects {
            for &(x, y) in &e.cells {
                if let Some(cell) = cell_at(buf, area, x, y) {
                    cell.set_char(e.ch).set_fg(e.color);
                }
            }
        }
        let mut put = |cx: i32, cy: i32, ch: char, fg: Color, bold: bool| {
            if let Some(cell) = cell_at(buf, area, cx, cy) {
                cell.set_char(ch).set_fg(fg);
                if bold {
                    cell.modifier.insert(Modifier::BOLD);
                }
            }
        };

        // Projectiles
        for p in g.projectiles() {
            let (x, y) = to_cell(p.pos);
            match p.kind {
                ProjectileKind::Bullet => put(x, y, '•', Color::LightYellow, false),
                ProjectileKind::Shell => put(x, y, 'o', Color::LightGreen, true),
            }
        }

        // Enemies: furthest along drawn last (on top).
        let mut enemies: Vec<&Enemy> = g.enemies().iter().collect();
        enemies.sort_by(|a, b| a.dist.total_cmp(&b.dist));
        for e in enemies {
            let (x, y) = to_cell(vec2(e.pos.x + e.lane, e.pos.y));
            let frac = e.hp / e.max_hp;
            let fg = if e.is_slowed() {
                Color::Rgb(220, 140, 255)
            } else if frac > 0.66 {
                Color::Rgb(255, 200, 120)
            } else if frac > 0.33 {
                Color::Rgb(255, 140, 60)
            } else {
                Color::Rgb(255, 60, 40)
            };
            let fg = if map.is_front(e.dist) { fg } else { dim(fg) };
            put(x, y, enemy_glyph(e.kind), fg, true);
        }

        // Cursor
        let style = if app.build.is_some() && g.can_place(app.build.unwrap(), cc, cr).is_ok() {
            Color::LightGreen
        } else if g.tower_at(cc, cr).is_some() {
            Color::LightCyan
        } else {
            Color::Gray
        };
        let (x0, y0) = (cc * CW, cr * CH);
        for (dx, dy, ch) in [(0, 0, '┌'), (CW - 1, 0, '┐'), (0, CH - 1, '└'), (CW - 1, CH - 1, '┘')] {
            put(x0 + dx, y0 + dy, ch, style, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_whole_game_on_autopilot() {
        let mut app = App::new();
        app.autopilot = true;
        app.speed = 4;
        let mut frames = 0;
        let mut snapshot = String::new();
        while !app.game.is_over() && frames < 200_000 {
            app.update();
            // Mouse/keyboard while playing must never panic.
            app.cursor = (frames % 10, frames / 10 % 12);
            if frames % 7 == 0 {
                app.handle(TermEvent::Key(KeyCode::Char('u').into()));
            }
            if frames % 50 == 0 {
                let s = render(&mut app, 100, 30);
                if app.game.wave() == 6 && snapshot.is_empty() && app.game.enemies().len() > 5 {
                    snapshot = s;
                }
            }
            frames += 1;
        }
        assert!(app.game.is_over(), "autopilot game never ended");
        println!("{snapshot}\n\n{}", render(&mut app, 100, 30));
    }

    #[test]
    fn tiny_terminal_shows_warning() {
        let mut app = App::new();
        assert!(render(&mut app, 40, 10).contains("too small"));
    }
}
