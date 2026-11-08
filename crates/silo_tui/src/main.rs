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
use silo_core::map::{COLS, FACILITIES, ROWS, SECTIONS, VIEW_ROWS, level_of_row, section_of_row, section_view_top};
use silo_core::stats::{MAX_TOWER_LEVEL, MAX_WAVES, START_SEALS};
use silo_core::*;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Each tile is drawn as CW x CH terminal cells.
const CW: i32 = 4;
const CH: i32 = 2;
const FIELD_W: u16 = (COLS * CW) as u16;
/// The field shows one section of the silo at a time.
const FIELD_H: u16 = (VIEW_ROWS * CH) as u16;
/// Height of the whole silo in cells.
const WORLD_H: i32 = ROWS * CH;
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
    /// Section shown in the field (0 = Up Top .. 2 = Down Deep).
    view: usize,
    build: Option<TowerKind>,
    /// Building barriers instead of towers.
    barrier_mode: bool,
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
            cursor: (2, ROWS - 2),
            view: SECTIONS.len() - 1,
            build: Some(TowerKind::Deputy),
            barrier_mode: false,
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
        app.say(Line::from("PgUp/PgDn (or [ ]) roll the view between the three sections."));
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
            Event::Won => self.say(Line::from("SILO 12 HOLDS. The Pact endures.").green().bold()),
            Event::Lost => self.say(Line::from("THE AIRLOCK IS OPEN. They see the outside.").red().bold()),
            Event::EnemyShot { from, to, .. } => {
                let n = (from.distance(to) * 8.0).ceil() as i32;
                let line = (1..n).map(|i| to_cell(from.lerp(to, i as f32 / n as f32))).collect();
                self.effect(line, '·', Color::Rgb(255, 160, 60), 2);
            }
            Event::TowerDestroyed { kind, pos, .. } => {
                self.effect(vec![to_cell(pos)], 'X', Color::Rgb(255, 120, 60), 12);
                self.say(Line::from(format!("Armed rebels destroyed a {}!", kind.name())).red());
            }
            Event::ItCollapsed { seconds } => self.say(
                Line::from(format!("IT HAS FALLEN. No cameras below Up Top for {seconds:.0}s.")).red().bold(),
            ),
            Event::SupplyJoined => self.say(
                Line::from("SUPPLY JOINS THE UPRISING. Rebels passing the depot get guns and tools.").red().bold(),
            ),
            Event::Equipped { pos, gun, .. } => {
                self.effect(vec![to_cell(pos)], if gun { '¬' } else { 'T' }, Color::Rgb(240, 150, 80), 10)
            }
            Event::BarrierBroken { pos, .. } => {
                self.effect(vec![to_cell(pos)], '#', Color::Rgb(255, 160, 90), 12);
                self.say(Line::from("A barrier was smashed!").yellow());
            }
            Event::ItRestored => self.say(Line::from("IT back online. Cameras restored.").cyan()),
            Event::SentToCleaning { seals, .. } if seals > 0 => {
                self.say(Line::from(format!("Sent to cleaning: +{seals} seal(s).")).green())
            }
            Event::PowerCut { first, seconds, .. } => {
                let name = SECTIONS[section_of_row(first)].0;
                self.say(Line::from(format!("Main power cut: {name} is dark for {seconds:.0}s.")).yellow());
            }
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
        if let Some(b) = self.game.barrier_at(c, r) {
            let msg = if b.broken {
                "Barrier wrecked; IT rebuilds it after the wave.".to_string()
            } else {
                format!(
                    "Barrier L{} {:.0}/{:.0}. [u] upgrades, [x] removes it for {}.",
                    b.level,
                    b.hp,
                    b.max_hp,
                    b.refund()
                )
            };
            self.say(Line::from(msg));
        } else if self.game.tower_at(c, r).is_some() {
            self.upgrade_at_cursor();
        } else if self.barrier_mode {
            let res = self.game.place_barrier(c, r).map(|_| ());
            self.report(res);
        } else if let Some(kind) = self.build {
            let res = self.game.place(kind, c, r).map(|_| ());
            self.report(res);
        } else {
            self.say(Line::from("Pick a tower with 1-4 first.").dark_gray());
        }
    }

    fn upgrade_at_cursor(&mut self) {
        let res = if let Some(b) = self.game.barrier_at(self.cursor.0, self.cursor.1) {
            let id = b.id;
            self.game.upgrade_barrier(id).map(|_| ())
        } else {
            match self.game.tower_at(self.cursor.0, self.cursor.1) {
                Some(t) => self.game.upgrade(t.id).map(|_| ()),
                None => Err(CommandError::NoSuchTower),
            }
        };
        self.report(res);
    }

    fn sell_at_cursor(&mut self) {
        if let Some(id) = self.game.barrier_at(self.cursor.0, self.cursor.1).map(|b| b.id) {
            let res = self.game.remove_barrier(id).map(|v| self.say(Line::from(format!("Barrier removed, +{v}."))));
            self.report(res);
            return;
        }
        let res = match self.game.tower_at(self.cursor.0, self.cursor.1) {
            Some(t) => self.game.sell(t.id).map(|v| self.say(Line::from(format!("Sold for {v}.")))),
            None => Err(CommandError::NoSuchTower),
        };
        self.report(res);
    }

    fn view_top(&self) -> i32 {
        section_view_top(self.view)
    }

    /// Roll the view one section up (-1) or down (+1); the cursor moves along.
    fn scroll(&mut self, dir: i32) {
        let next = (self.view as i32 + dir).clamp(0, SECTIONS.len() as i32 - 1) as usize;
        if next != self.view {
            let dy = section_view_top(next) - self.view_top();
            self.cursor.1 = (self.cursor.1 + dy).clamp(0, ROWS - 1);
            self.view = next;
        }
    }

    /// Keep the cursor visible: switch sections when it walks off the top or bottom.
    fn follow_cursor(&mut self) {
        let r = self.cursor.1;
        if r < self.view_top() || r >= self.view_top() + VIEW_ROWS {
            self.view = section_of_row(r);
        }
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
                        self.barrier_mode = false;
                    }
                    KeyCode::Char('5') | KeyCode::Char('b') => {
                        self.barrier_mode = !self.barrier_mode;
                        self.build = None;
                    }
                    KeyCode::Esc => {
                        self.build = None;
                        self.barrier_mode = false;
                    }
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
                    KeyCode::PageUp | KeyCode::Char('[') => self.scroll(-1),
                    KeyCode::PageDown | KeyCode::Char(']') => self.scroll(1),
                    KeyCode::Char('r') if self.game.is_over() => *self = App::new(),
                    _ => {}
                }
                self.follow_cursor();
            }
            TermEvent::Mouse(m) => {
                let f = self.field_rect;
                let (x, y) = (m.column, m.row);
                if x < f.x || y < f.y || x >= f.x + f.width || y >= f.y + f.height {
                    return;
                }
                let tile = (((x - f.x) as i32) / CW, ((y - f.y) as i32) / CH + self.view_top());
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
                    MouseEventKind::ScrollUp => self.scroll(-1),
                    MouseEventKind::ScrollDown => self.scroll(1),
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

        let (name, first, last) = SECTIONS[self.view];
        let title = Line::from(vec![
            " THE STAIRWELL · ".bold(),
            name.yellow().bold(),
            format!(" levels {}-{} ", level_of_row(first), level_of_row(last) + 3).dark_gray(),
        ])
        .centered();
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
                ("5", "barrier"),
                ("u", "upgrade"),
                ("x", "sell"),
                ("PgUp/Dn", "view"),
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
            (" SILO 12 HOLDS ", Color::Green, "The Pact is preserved.\nThe outside stays a secret.")
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

    fn it_line(&self) -> Line<'static> {
        let g = &self.game;
        if g.cameras_down() {
            return Line::from(vec![
                "IT    ".dark_gray(),
                format!("DOWN {:.0}s — no cameras below Up Top", g.it_down().ceil()).red().bold(),
            ]);
        }
        let n = (g.it_hp() / stats::IT_HP * 10.0).ceil() as usize;
        Line::from(vec![
            "IT    ".dark_gray(),
            Span::styled(format!("{:<10}", "▮".repeat(n)), Style::new().fg(Color::Rgb(90, 160, 255))),
            format!(" server room {:.0}/{:.0}", g.it_hp(), stats::IT_HP).dark_gray(),
        ])
    }

    /// "View" line: the three sections, the visible one highlighted, with rebel counts.
    fn sections_line(&self) -> Line<'static> {
        let mut counts = [0usize; SECTIONS.len()];
        for e in self.game.enemies().iter().filter(|e| self.game.is_visible(e.pos)) {
            counts[section_of_row(e.pos.y.floor() as i32)] += 1;
        }
        let mut spans = vec!["View  ".dark_gray()];
        for (i, (name, ..)) in SECTIONS.iter().enumerate() {
            let label = format!(" {name} ");
            spans.push(if i == self.view { label.black().on_yellow() } else { label.gray() });
            spans.push(if counts[i] > 0 { format!("{} ", counts[i]).red().bold() } else { "  ".into() });
        }
        Line::from(spans)
    }

    fn draw_panel(&self, f: &mut ratatui::Frame, area: Rect) {
        let g = &self.game;
        let [status, shop, info, log] = Layout::vertical([
            Constraint::Length(9),
            Constraint::Length(7),
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
            self.it_line(),
            self.sections_line(),
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
        let mut shop_lines = shop_lines;
        let style = if self.barrier_mode {
            Style::new().fg(Color::Black).bg(BARRIER_FG)
        } else if g.chits() >= stats::BARRIER_COST {
            Style::new().fg(BARRIER_FG)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        shop_lines.push(Line::from(vec![
            Span::styled(format!(" 5 ▓ {:<12} {:>4} ", "Barrier", stats::BARRIER_COST), style),
            " Blocks the stair. Only above Supply.".dark_gray(),
        ]));
        f.render_widget(Paragraph::new(shop_lines).block(Block::bordered().title(" ARMORY ")), shop);

        // Cursor info
        let (c, r) = self.cursor;
        let mut lines = vec![];
        if let Some(b) = g.barrier_at(c, r) {
            lines.push(Line::from(Span::styled("Barrier", Style::new().fg(BARRIER_FG).bold())));
            if b.broken {
                lines.push(Line::from("wrecked: rebuilt after the wave".red()));
            } else {
                lines.push(Line::from(format!("level {}  hp {:.0}/{:.0}", b.level, b.hp, b.max_hp)));
                if let Some(c) = b.upgrade_cost() {
                    lines.push(Line::from(vec!["[u] upgrade for ".into(), format!("{c}").yellow()]));
                }
            }
            lines.push(Line::from("Bare hands barely dent it; tools from Supply smash it.".dark_gray()));
            lines.push(Line::from(vec!["[x] remove for ".into(), format!("{}", b.refund()).yellow()]));
        } else if let Some(t) = g.tower_at(c, r) {
            let s = t.stats();
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", t.kind.name()), Style::new().fg(tower_color(t.kind)).bold()),
                format!("Lv {}/{MAX_TOWER_LEVEL}", t.level).into(),
                format!("   kills {}", t.kills).dark_gray(),
                format!("   hp {:.0}/{:.0}", t.hp, t.max_hp).into(),
            ]));
            lines.push(Line::from(format!(
                "dmg {:.0}  reach {:.1}  every {:.2}s{}",
                s.damage,
                t.reach,
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
                        format!(" → dmg {:.0} reach {:.1}", n.damage, map::reach(t.pos, n.range)).dark_gray(),
                    ])
                }
                None => Line::from("max level".dark_gray()),
            });
            lines.push(Line::from(vec!["[x] sell for ".into(), format!("{}", t.sell_value()).yellow()]));
        } else {
            let place = map::facility_at(c, r).map_or(String::new(), |f| format!("{f}, "));
            let where_ = format!(" ({place}level ~{}, {})", level_of_row(r), SECTIONS[section_of_row(r)].0);
            let desc = match g.map().tile(c, r) {
                Tile::Floor => format!("Living quarters{where_}. Deputies, marksmen and jammers."),
                Tile::ItRoom => "IT's server room. If it falls, the cameras below Up Top go dark.".into(),
                Tile::SupplyRoom if g.supply_joined() => {
                    "Supply depot. Supply has joined the uprising: guns and tools for every rebel passing.".into()
                }
                Tile::SupplyRoom => "Supply depot. Still loyal... for now.".into(),
                Tile::PowerRoom => "The main generator. Every uprising starts down here.".into(),
                Tile::Shaft => format!("Open stairwell{where_}. Nothing stands in mid-air."),
                Tile::Stairs => "The spiral stair. The uprising climbs here. Deputies and gas launchers.".into(),
                Tile::Rock => "Silo wall. Solid concrete and earth.".into(),
                Tile::Airlock => "The airlock. Do not let it open.".into(),
                Tile::Outside => "The outside. Toxic. Nobody goes there.".into(),
                Tile::OffMap => String::new(),
            };
            lines.push(Line::from(desc));
            if self.barrier_mode {
                lines.push(match g.can_place_barrier(c, r) {
                    Ok(()) => Line::from(format!("[⏎] build barrier for {}", stats::BARRIER_COST)).green(),
                    Err(e) => Line::from(format!("{e}")).red(),
                });
            } else if let (Some(k), true) = (self.build, g.map().buildable(c, r)) {
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

/// Cell at world-cell (x, y) given the view's top cell row `oy`, or None if not visible.
fn cell_at(buf: &mut Buffer, area: Rect, oy: i32, x: i32, y: i32) -> Option<&mut ratatui::buffer::Cell> {
    let y = y - oy;
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
const BARRIER_FG: Color = Color::Rgb(200, 160, 110);
const STAIR_BACK: Color = Color::Rgb(92, 68, 50);

/// Cheap deterministic per-cell noise for textures.
fn noise(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 13;
    h.wrapping_mul(0xC2B2_AE3D) >> 24
}

/// Draw one pass of the spiral stair. Half-block glyphs give 2 sub-rows per cell, so the
/// diagonal reads as a run of steps.
fn draw_stair(buf: &mut Buffer, area: Rect, oy: i32, a: Vec2, b: Vec2, fg: Color) {
    if (a.x - b.x).abs() < 1e-3 {
        // Vertical hatch ladder up into the airlock.
        let x = (a.x * CW as f32).floor() as i32;
        let (y0, y1) = ((b.y * CH as f32) as i32, (a.y * CH as f32) as i32);
        for y in y0..=y1 {
            for (dx, ch) in [(-1, '╟'), (0, '╢')] {
                if let Some(c) = cell_at(buf, area, oy, x + dx, y) {
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
        if let Some(c) = cell_at(buf, area, oy, x, y) {
            c.set_char(ch).set_fg(fg);
        }
    }
    // Landings at the turns.
    for p in [a, b] {
        let (px, py) = to_cell(p);
        for dx in -1..=0 {
            if let Some(c) = cell_at(buf, area, oy, px + dx, py) {
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

/// Fits a facility name into a two-tile room.
fn section_short(name: &str) -> String {
    name.chars().take((2 * CW) as usize).collect()
}

impl Widget for FieldView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let app = self.app;
        let g = &app.game;
        let map = g.map();
        let oy = app.view_top() * CH;
        let put = |buf: &mut Buffer, cx: i32, cy: i32, ch: char, style: Style| {
            if let Some(cell) = cell_at(buf, area, oy, cx, cy) {
                cell.set_char(ch).set_style(style);
            }
        };
        let lost = g.phase() == Phase::Lost;
        let silo_x = (CW, (COLS - 1) * CW); // inner wall span in cells

        // Background, per cell.
        for y in oy..oy + FIELD_H as i32 {
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
                    Tile::ItRoom => (' ', Style::new().bg(Color::Rgb(16, 24, 40))),
                    Tile::SupplyRoom => (' ', Style::new().bg(Color::Rgb(40, 32, 22))),
                    Tile::PowerRoom => ('≈', Style::new().fg(Color::Rgb(230, 190, 40)).bg(Color::Rgb(40, 30, 14))),
                    Tile::OffMap => continue,
                };
                put(buf, x, y, ch, style);
            }
        }

        // Concrete inner face of the cylinder wall, curving in at the dome.
        let concrete = Style::new().bg(ROCK_BG).fg(Color::Rgb(120, 122, 126));
        for y in CH..WORLD_H {
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
        for row in (2..ROWS).step_by(2) {
            for (dx, ch) in format!("{:>3}", level_of_row(row)).chars().enumerate() {
                put(buf, dx as i32, row * CH, ch, label);
            }
        }
        // Facility names painted on the first floor of each block, both sides of the shaft.
        let sign = Style::new().fg(Color::Rgb(150, 130, 90)).italic();
        for &(row, _, left, right) in FACILITIES {
            for (x0, name) in [(CW, left), (9 * CW, right)] {
                if row == map::IT_ROOM[0].1 && x0 == CW || row == map::SUPPLY_ROOM[0].1 && x0 != CW {
                    continue; // these rooms draw their own signs
                }
                for (dx, ch) in section_short(name).chars().enumerate() {
                    put(buf, x0 + dx as i32, row * CH, ch, sign.bg(FLOOR_BG));
                }
            }
        }

        // The spiral: back passes, then the central pillar, then front passes on top.
        for (a, b, front) in map.segments() {
            if !front {
                draw_stair(buf, area, oy, a, b, STAIR_BACK);
            }
        }
        let px = (map::PILLAR_X * CW as f32) as i32;
        for y in CH..WORLD_H {
            for (dx, ch) in [(-1, '▐'), (0, '▌')] {
                if let Some(c) = cell_at(buf, area, oy, px + dx, y).filter(|c| c.symbol() == " ") {
                    c.set_char(ch).set_fg(PILLAR_FG);
                }
            }
        }
        for (a, b, front) in map.segments() {
            if front {
                draw_stair(buf, area, oy, a, b, STAIR_FRONT);
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
            .map(|t| t.reach)
            .or_else(|| {
                app.build
                    .filter(|&k| map.tower_allowed(k, cc, cr))
                    .map(|k| map::reach(map.tower_pos(cc, cr), k.stats(1).range))
            });
        if let Some(range) = preview {
            for cell in cells_in_circle(map.tower_pos(cc, cr), range) {
                if let Some(c) = cell_at(buf, area, oy, cell.0, cell.1) {
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
            if t.hp < t.max_hp {
                let f = t.hp / t.max_hp;
                let c = if f > 0.5 { Color::Green } else if f > 0.25 { Color::Yellow } else { Color::Red };
                put(buf, x0, y0 + 1, '♥', Style::new().bg(Color::Rgb(20, 22, 26)).fg(c));
            }
        }

        // Supply depot.
        {
            let (c0, r) = map::SUPPLY_ROOM[0];
            let (x0, y0) = (c0 * CW, r * CH);
            let joined = g.supply_joined();
            let room = Style::new().bg(Color::Rgb(40, 32, 22));
            let fg = if joined { Color::Rgb(240, 100, 70) } else { Color::Rgb(220, 184, 92) };
            for (dx, ch) in (if joined { "SUPPLY ⚑" } else { "SUPPLY  " }).chars().enumerate() {
                put(buf, x0 + dx as i32, y0, ch, room.fg(fg).bold());
            }
            let shelves = ["╔══╗╔══╗", "▤▤ ▤▤▤ ▤", if joined { "▤ ¬ ▤▤ ¬" } else { "▤▤▤▤▤▤▤▤" }];
            for (dy, row) in shelves.iter().enumerate() {
                for (dx, ch) in row.chars().enumerate() {
                    put(buf, x0 + dx as i32, y0 + 1 + dy as i32, ch, room.fg(Color::Rgb(150, 115, 70)));
                }
            }
        }

        // Barriers across the stair.
        for b in g.barriers() {
            let (x, y) = to_cell(b.pos);
            let f = b.hp / b.max_hp;
            let ch = if b.broken {
                '·'
            } else if f > 0.66 {
                '▓'
            } else if f > 0.33 {
                '▒'
            } else {
                '░'
            };
            for dx in -1..=1 {
                put(buf, x + dx, y, ch, Style::new().fg(BARRIER_FG).bg(Color::Rgb(50, 38, 24)).bold());
            }
        }

        // IT's server room.
        {
            let (c0, r) = map::IT_ROOM[0];
            let (x0, y0) = (c0 * CW, r * CH);
            let w = 2 * CW;
            let room = Style::new().bg(Color::Rgb(16, 24, 40));
            let (top, top_fg) = if g.cameras_down() {
                (format!("IT {:>3.0}s", g.it_down().ceil()), Color::Red)
            } else {
                let n = (g.it_hp() / stats::IT_HP * 5.0).ceil() as usize;
                (format!("IT{:<5}", "▮".repeat(n)), Color::Rgb(110, 170, 240))
            };
            for (dx, ch) in top.chars().take(w as usize).enumerate() {
                put(buf, x0 + dx as i32, y0, ch, room.fg(top_fg).bold());
            }
            for dy in 1..2 * CH {
                for dx in 0..w {
                    let lit = !g.cameras_down() && noise(dx + 31 * dy, (g.tick() / 8) as i32) < 150;
                    let fg = if lit { Color::Rgb(90, 240, 120) } else { Color::Rgb(50, 54, 62) };
                    put(buf, x0 + dx, y0 + dy, if dx % 2 == 0 { '▥' } else { '▤' }, room.fg(fg));
                }
            }
        }

        // What IT can't see: whole sections without power, or everything below Up Top while
        // the server room is down. Drawn over the towers; rebels and shots there are skipped.
        for y in oy..oy + FIELD_H as i32 {
            let row = y / CH;
            if !g.is_row_dark(row) {
                continue;
            }
            let no_signal = g.cameras_down() && row > SECTIONS[0].2;
            for x in 0..FIELD_W as i32 {
                let (ch, st) = if no_signal {
                    let n = noise(x, y ^ (g.tick() / 3) as i32);
                    let ch = if n < 40 { '░' } else if n < 60 { '▒' } else { ' ' };
                    (ch, Style::new().bg(Color::Rgb(8, 8, 10)).fg(Color::Rgb(90, 90, 90)))
                } else if !(CW..(COLS - 1) * CW).contains(&x) {
                    continue; // the earth outside the silo is unaffected
                } else {
                    (' ', Style::new().bg(Color::Rgb(4, 4, 6)))
                };
                put(buf, x, y, ch, st);
            }
        }
        // Label each dark section that is on screen.
        for (i, &(name, first, last)) in SECTIONS.iter().enumerate() {
            if !g.is_row_dark(first) {
                continue;
            }
            let (msg, fg) = if g.cameras_down() && i > 0 {
                (format!(" NO SIGNAL · IT DOWN {:.0}s ", g.it_down().ceil()), Color::Red)
            } else {
                (format!(" NO POWER · {name} IS DARK "), Color::Rgb(170, 150, 100))
            };
            let mid = (first + last + 1) * CH / 2;
            let y = mid.clamp(oy + 1, oy + FIELD_H as i32 - 2);
            if !(first * CH..=(last + 1) * CH).contains(&y) {
                continue;
            }
            let x0 = (FIELD_W as i32 - msg.chars().count() as i32) / 2;
            for (dx, ch) in msg.chars().enumerate() {
                put(buf, x0 + dx as i32, y, ch, Style::new().bg(Color::Black).fg(fg).bold());
            }
        }

        // Effects (below enemies)
        for e in &app.effects {
            for &(x, y) in &e.cells {
                if g.is_row_dark(y / CH) {
                    continue;
                }
                if let Some(cell) = cell_at(buf, area, oy, x, y) {
                    cell.set_char(e.ch).set_fg(e.color);
                }
            }
        }
        let mut put = |cx: i32, cy: i32, ch: char, fg: Color, bold: bool| {
            if let Some(cell) = cell_at(buf, area, oy, cx, cy) {
                cell.set_char(ch).set_fg(fg);
                if bold {
                    cell.modifier.insert(Modifier::BOLD);
                }
            }
        };

        // Projectiles
        for p in g.projectiles().iter().filter(|p| g.is_visible(p.pos)) {
            let (x, y) = to_cell(p.pos);
            match p.kind {
                ProjectileKind::Bullet => put(x, y, '•', Color::LightYellow, false),
                ProjectileKind::Shell => put(x, y, 'o', Color::LightGreen, true),
            }
        }

        // Enemies: furthest along drawn last (on top).
        let mut enemies: Vec<&Enemy> = g.enemies().iter().filter(|e| g.is_visible(e.pos)).collect();
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
            // Supply's guns show as green, tools as cyan.
            let fg = if e.armed {
                Color::Rgb(150, 230, 110)
            } else if e.tools {
                Color::Rgb(120, 220, 230)
            } else {
                fg
            };
            let fg = if map.is_front(e.dist) { fg } else { dim(fg) };
            put(x, y, enemy_glyph(e.kind), fg, true);
        }

        // Cursor
        let style = if (app.build.is_some() && g.can_place(app.build.unwrap(), cc, cr).is_ok())
            || (app.barrier_mode && g.can_place_barrier(cc, cr).is_ok())
        {
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
    fn scrolling_between_sections() {
        let mut app = App::new();
        assert_eq!(app.view, 2, "starts where the uprising starts");
        let deep = render(&mut app, 100, 30);
        assert!(deep.contains("DOWN DEEP") && deep.contains("MECH"));
        app.handle(TermEvent::Key(KeyCode::PageUp.into()));
        assert_eq!(app.view, 1);
        assert_eq!(app.cursor.1, ROWS - 2 - 12, "cursor rolls with the view");
        app.handle(TermEvent::Key(KeyCode::PageUp.into()));
        app.handle(TermEvent::Key(KeyCode::PageUp.into()));
        assert_eq!(app.view, 0, "clamped at the top");
        let top = render(&mut app, 100, 30);
        assert!(top.contains("UP TOP") && top.contains("◖████◗"), "airlock visible up top");
        // Walking the cursor off the bottom of the view rolls down a section.
        app.cursor = (2, section_view_top(0) + VIEW_ROWS - 1);
        app.handle(TermEvent::Key(KeyCode::Down.into()));
        assert_eq!(app.view, 1);
        // Clicking in the mids view targets mids rows.
        render(&mut app, 100, 30);
        let f = app.field_rect;
        app.handle(TermEvent::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: MouseEventKind::Moved,
            column: f.x + 2,
            row: f.y + 2 * CH as u16,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        }));
        assert_eq!(app.cursor, (0, section_view_top(1) + 2));
    }

    #[test]
    fn power_cut_blacks_out_a_section() {
        let mut app = App::new();
        app.game = Game::new(5);
        app.bot = Bot::balanced(app.game.map());
        app.autopilot = true;
        let mut ticks = 0;
        while app.game.blackouts().is_empty() && !app.game.is_over() && ticks < 3_000_000 {
            app.update();
            ticks += 1;
        }
        let b = app.game.blackouts()[0];
        app.view = section_of_row(b.first);
        let out = render(&mut app, 100, 30);
        println!("{out}");
        assert!(out.contains("NO POWER"), "dark section not labelled");
        // No rebel glyphs inside the dark band.
        let f = app.field_rect;
        let lines: Vec<&str> = out.lines().collect();
        for row in b.first.max(app.view_top())..=b.last.min(app.view_top() + VIEW_ROWS - 1) {
            for sub in 0..CH {
                let y = f.y as i32 + (row - app.view_top()) * CH + sub;
                let line: String = lines[y as usize].chars().skip(f.x as usize + CW as usize).take(FIELD_W as usize - 2 * CW as usize).collect();
                assert!(!line.chars().any(|c| "pmM&".contains(c)) || line.contains("NO POWER") || line.contains("DARK"), "rebel visible in the dark: {line}");
            }
        }
    }

    #[test]
    fn barrier_via_keys() {
        let mut app = App::new();
        app.view = 1;
        app.cursor = (4, 20);
        app.handle(TermEvent::Key(KeyCode::Char('5').into()));
        assert!(app.barrier_mode);
        app.handle(TermEvent::Key(KeyCode::Enter.into()));
        assert!(app.game.barrier_at(4, 20).is_some(), "barrier placed");
        let out = render(&mut app, 100, 30);
        assert!(out.contains("▓▓"), "barrier drawn:\n{out}");
        // Below Supply is refused.
        app.cursor = (4, 32);
        app.handle(TermEvent::Key(KeyCode::Enter.into()));
        assert!(app.game.barrier_at(4, 32).is_none());
        app.cursor = (4, 20);
        app.handle(TermEvent::Key(KeyCode::Char('x').into()));
        assert!(app.game.barriers().is_empty(), "barrier removed");
    }

    #[test]
    fn tiny_terminal_shows_warning() {
        let mut app = App::new();
        assert!(render(&mut app, 40, 10).contains("too small"));
    }
}
