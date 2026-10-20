// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Application state, key handling and background fetching.

use crate::api::Api;
use crate::config::Config;
use crate::game::{Cursor, LogLine, play_log};
use crate::model::{GameFeed, ScheduleGame, Standings};
use anyhow::Result;
use chrono::{Duration as Days, Local, NaiveDate, Timelike};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

pub enum Msg {
    Schedule(NaiveDate, Result<Vec<ScheduleGame>>),
    Feed(u64, Box<Result<GameFeed>>),
    Standings(StandingsKind, Result<Standings>),
    /// Nearest day with games, searching forward (`true`) or back.
    GameDay(bool, Result<Option<NaiveDate>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Scoreboard,
    Standings,
    Game,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandingsKind {
    Division,
    WildCard,
}

impl StandingsKind {
    pub fn api_name(self) -> &'static str {
        match self {
            Self::Division => "regularSeason",
            Self::WildCard => "wildCard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameTab {
    Gamecast,
    Box,
    Scorecard,
    Plays,
}

impl GameTab {
    pub const ALL: [GameTab; 4] = [Self::Gamecast, Self::Box, Self::Scorecard, Self::Plays];
    pub fn label(self) -> &'static str {
        match self {
            Self::Gamecast => "Gamecast",
            Self::Box => "Box score",
            Self::Scorecard => "Scorecard",
            Self::Plays => "Plays",
        }
    }
    pub fn key(self) -> char {
        match self {
            Self::Gamecast => 'c',
            Self::Box => 'b',
            Self::Scorecard => 's',
            Self::Plays => 'p',
        }
    }
}

/// How far `[` / `]` look for a day with games.
pub const SEARCH_DAYS: i64 = 150;

pub const SPEEDS: [f64; 7] = [0.5, 1.0, 1.5, 2.0, 3.0, 5.0, 10.0];

pub struct Replay {
    pub speed_idx: usize,
    pub playing: bool,
    pub last_step: Instant,
}

pub struct GameView {
    pub pk: u64,
    pub feed: Option<GameFeed>,
    pub error: Option<String>,
    pub loading: bool,
    pub last_fetch: Option<Instant>,
    pub tab: GameTab,
    /// `None` follows the latest event; `Some` is a fixed point in the game.
    pub cursor: Option<Cursor>,
    pub replay: Replay,
    /// Show the home team on the box score and scorecard.
    pub home_side: bool,
    pub scroll: u16,
    pub hscroll: u16,
    pub plays_sel: usize,
    /// Number of log lines in the last drawn plays list (for clamping).
    pub plays_len: usize,
}

impl GameView {
    fn new(pk: u64) -> Self {
        Self {
            pk,
            feed: None,
            error: None,
            loading: false,
            last_fetch: None,
            tab: GameTab::Gamecast,
            cursor: None,
            replay: Replay {
                speed_idx: 1,
                playing: false,
                last_step: Instant::now(),
            },
            home_side: false,
            scroll: 0,
            hscroll: 0,
            plays_sel: 0,
            plays_len: 0,
        }
    }

    pub fn plays(&self) -> &[crate::model::Play] {
        self.feed
            .as_ref()
            .map(|f| f.live_data.plays.all_plays.as_slice())
            .unwrap_or(&[])
    }

    /// The point in the game being shown.
    pub fn at(&self) -> Option<Cursor> {
        let plays = self.plays();
        match self.cursor {
            Some(c) if c.play < plays.len() => Some(c),
            _ => Cursor::end(plays),
        }
    }

    pub fn following(&self) -> bool {
        self.cursor.is_none()
    }

    pub fn is_live(&self) -> bool {
        self.feed
            .as_ref()
            .is_some_and(|f| f.game_data.status.is_live())
    }

    fn set_cursor(&mut self, c: Option<Cursor>) {
        let plays = self.plays();
        // Landing on the very end of a game means following it.
        self.cursor = match c {
            Some(c) if Some(c) == Cursor::end(plays) => None,
            other => other,
        };
    }

    pub fn log(&self) -> Vec<LogLine> {
        match self.at() {
            Some(c) => play_log(self.plays(), c),
            None => Vec::new(),
        }
    }
}

pub struct App {
    pub cfg: Config,
    api: Api,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub screen: Screen,
    pub prev_screen: Screen,
    pub date: NaiveDate,
    pub games: Option<Vec<ScheduleGame>>,
    pub sched_error: Option<String>,
    pub sched_loading: bool,
    sched_fetched: Option<Instant>,
    pub sel: usize,
    pub standings_kind: StandingsKind,
    pub standings: HashMap<StandingsKind, Standings>,
    pub standings_error: Option<String>,
    standings_loading: bool,
    pub standings_scroll: u16,
    pub game: Option<GameView>,
    pub help: bool,
    pub quit: bool,
    pending_g: bool,
    pub status: Option<(String, Instant)>,
    pub last_title: String,
}

pub fn baseball_today(rollover_hour: u32) -> NaiveDate {
    let now = Local::now();
    let d = now.date_naive();
    if now.hour() < rollover_hour {
        d - Days::days(1)
    } else {
        d
    }
}

impl App {
    pub fn new(cfg: Config, api: Api, date: Option<NaiveDate>, game: Option<u64>) -> Self {
        let (tx, rx) = channel();
        let date = date.unwrap_or_else(|| baseball_today(cfg.day_rollover_hour));
        let mut app = Self {
            cfg,
            api,
            tx,
            rx,
            screen: Screen::Scoreboard,
            prev_screen: Screen::Scoreboard,
            date,
            games: None,
            sched_error: None,
            sched_loading: false,
            sched_fetched: None,
            sel: 0,
            standings_kind: StandingsKind::Division,
            standings: HashMap::new(),
            standings_error: None,
            standings_loading: false,
            standings_scroll: 0,
            game: None,
            help: false,
            quit: false,
            pending_g: false,
            status: None,
            last_title: String::new(),
        };
        app.fetch_schedule();
        if let Some(pk) = game {
            app.open_game(pk);
        }
        app
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    fn fetch_schedule(&mut self) {
        if self.sched_loading {
            return;
        }
        self.sched_loading = true;
        let (tx, api, date) = (self.tx.clone(), self.api.clone(), self.date);
        thread::spawn(move || {
            let r = api
                .schedule(&date.format("%Y-%m-%d").to_string())
                .map(|s| s.dates.into_iter().flat_map(|d| d.games).collect());
            let _ = tx.send(Msg::Schedule(date, r));
        });
    }

    fn fetch_feed(&mut self) {
        let Some(g) = self.game.as_mut() else { return };
        if g.loading {
            return;
        }
        g.loading = true;
        let (tx, api, pk) = (self.tx.clone(), self.api.clone(), g.pk);
        thread::spawn(move || {
            let _ = tx.send(Msg::Feed(pk, Box::new(api.feed(pk))));
        });
    }

    fn fetch_standings(&mut self) {
        if self.standings_loading {
            return;
        }
        self.standings_loading = true;
        let kind = self.standings_kind;
        let season = self.date.format("%Y").to_string().parse().unwrap_or(2026);
        let (tx, api) = (self.tx.clone(), self.api.clone());
        thread::spawn(move || {
            let _ = tx.send(Msg::Standings(kind, api.standings(season, kind.api_name())));
        });
    }

    fn open_game(&mut self, pk: u64) {
        if self.game.as_ref().map(|g| g.pk) != Some(pk) {
            self.game = Some(GameView::new(pk));
            self.fetch_feed();
        }
        if self.screen != Screen::Game {
            self.prev_screen = self.screen;
        }
        self.screen = Screen::Game;
    }

    fn sorted_games(&mut self) {
        let favs = self.cfg.favorites.clone();
        if let Some(games) = self.games.as_mut() {
            games.sort_by_key(|g| {
                let fav = favs.iter().any(|f| {
                    f == &g.teams.away.team.abbreviation || f == &g.teams.home.team.abbreviation
                });
                (!fav, g.game_date.clone(), g.game_pk)
            });
        }
    }

    pub fn handle_msgs(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Schedule(date, r) => {
                    self.sched_loading = false;
                    if date != self.date {
                        // A newer date was picked while this was loading.
                        self.fetch_schedule();
                        continue;
                    }
                    self.sched_fetched = Some(Instant::now());
                    match r {
                        Ok(g) => {
                            let keep = self.selected_game().map(|g| g.game_pk);
                            self.games = Some(g);
                            self.sched_error = None;
                            self.sorted_games();
                            if let Some(pk) = keep
                                && let Some(i) = self
                                    .games
                                    .as_ref()
                                    .and_then(|gs| gs.iter().position(|g| g.game_pk == pk))
                            {
                                self.sel = i;
                            }
                            let n = self.games.as_ref().map_or(0, |g| g.len());
                            self.sel = self.sel.min(n.saturating_sub(1));
                        }
                        Err(e) => self.sched_error = Some(format!("{e:#}")),
                    }
                }
                Msg::Feed(pk, r) => {
                    let Some(g) = self.game.as_mut() else {
                        continue;
                    };
                    if g.pk != pk {
                        continue;
                    }
                    g.loading = false;
                    g.last_fetch = Some(Instant::now());
                    match *r {
                        Ok(feed) => {
                            g.feed = Some(feed);
                            g.error = None;
                        }
                        Err(e) => g.error = Some(format!("{e:#}")),
                    }
                }
                Msg::GameDay(forward, r) => match r {
                    Ok(Some(d)) => {
                        self.date = d;
                        self.change_date(0);
                    }
                    Ok(None) => self.flash(format!(
                        "No games in the {} {SEARCH_DAYS} days",
                        if forward { "next" } else { "last" }
                    )),
                    Err(e) => self.flash(format!("Search failed: {e:#}")),
                },
                Msg::Standings(kind, r) => {
                    self.standings_loading = false;
                    self.standings_error = None;
                    match r {
                        Ok(s) => {
                            self.standings.insert(kind, s);
                        }
                        Err(e) => self.standings_error = Some(format!("{e:#}")),
                    }
                }
            }
        }
    }

    /// Periodic work: polling live data and stepping replays.
    pub fn tick(&mut self) {
        if let Some((_, at)) = &self.status
            && at.elapsed() > Duration::from_secs(4)
        {
            self.status = None;
        }
        // Scoreboard refresh while anything on it is live (or about to be).
        let any_active = self.games.as_ref().is_some_and(|gs| {
            gs.iter()
                .any(|g| g.status.is_live() || g.status.detailed_state == "Warmup")
        });
        if any_active
            && self.screen != Screen::Game
            && self.sched_fetched.is_some_and(|t| {
                t.elapsed() >= Duration::from_secs(self.cfg.scoreboard_poll_seconds)
            })
        {
            self.fetch_schedule();
        }

        let live_poll = self.cfg.live_poll_seconds;
        let step = self.cfg.replay_step_seconds;
        let mut want_feed = false;
        if let Some(g) = self.game.as_mut() {
            let interval = match g.feed.as_ref().map(|f| &f.game_data.status) {
                Some(s) if s.is_final() => None,
                Some(s) if s.is_preview() && s.detailed_state != "Warmup" => Some(60),
                _ => Some(live_poll),
            };
            if let (Some(iv), Some(t)) = (interval, g.last_fetch)
                && self.screen == Screen::Game
                && t.elapsed() >= Duration::from_secs(iv)
            {
                want_feed = true;
            }
            if g.replay.playing {
                let dt = step / SPEEDS[g.replay.speed_idx];
                if g.replay.last_step.elapsed().as_secs_f64() >= dt {
                    g.replay.last_step = Instant::now();
                    let plays = g.plays();
                    match g.at().and_then(|c| c.next(plays)) {
                        Some(n) => g.set_cursor(Some(n)),
                        None => {
                            g.replay.playing = false;
                            g.cursor = None;
                        }
                    }
                }
            }
        }
        if want_feed {
            self.fetch_feed();
        }
    }

    pub fn wants_fast_ticks(&self) -> bool {
        self.game.as_ref().is_some_and(|g| g.replay.playing)
    }

    pub fn selected_game(&self) -> Option<&ScheduleGame> {
        self.games.as_ref()?.get(self.sel)
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let delta: i32 = match m.kind {
            MouseEventKind::ScrollDown => 3,
            MouseEventKind::ScrollUp => -3,
            _ => return,
        };
        match self.screen {
            Screen::Scoreboard => self.move_sel(delta.signum()),
            Screen::Standings => {
                self.standings_scroll = add(self.standings_scroll, delta);
            }
            Screen::Game => {
                if let Some(g) = self.game.as_mut() {
                    if g.tab == GameTab::Plays {
                        g.plays_sel = add_usize(g.plays_sel, delta.signum(), g.plays_len);
                    } else {
                        g.scroll = add(g.scroll, delta);
                    }
                }
            }
        }
    }

    fn move_sel(&mut self, d: i32) {
        let n = self.games.as_ref().map_or(0, |g| g.len());
        self.sel = add_usize(self.sel, d, n);
    }

    fn find_game_day(&mut self, forward: bool) {
        let (start, end) = if forward {
            (
                self.date + Days::days(1),
                self.date + Days::days(SEARCH_DAYS),
            )
        } else {
            (
                self.date - Days::days(SEARCH_DAYS),
                self.date - Days::days(1),
            )
        };
        let (tx, api) = (self.tx.clone(), self.api.clone());
        self.flash("Looking for games…");
        thread::spawn(move || {
            let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
            let r = api.game_dates(&fmt(start), &fmt(end)).map(|dates| {
                let pick = if forward { dates.first() } else { dates.last() };
                pick.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            });
            let _ = tx.send(Msg::GameDay(forward, r));
        });
    }

    fn change_date(&mut self, d: i64) {
        self.date += Days::days(d);
        self.games = None;
        self.sched_error = None;
        self.sel = 0;
        self.fetch_schedule();
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        let gg = std::mem::take(&mut self.pending_g);
        match k.code {
            KeyCode::Char('?') => {
                self.help = true;
                return;
            }
            KeyCode::Char('1') => {
                self.screen = Screen::Scoreboard;
                return;
            }
            KeyCode::Char('2') => {
                self.screen = Screen::Standings;
                if !self.standings.contains_key(&self.standings_kind) {
                    self.fetch_standings();
                }
                return;
            }
            KeyCode::Char('3') if self.game.is_some() => {
                self.screen = Screen::Game;
                return;
            }
            _ => {}
        }
        match self.screen {
            Screen::Scoreboard => self.key_scoreboard(k, gg),
            Screen::Standings => self.key_standings(k, gg, ctrl),
            Screen::Game => self.key_game(k, gg, ctrl),
        }
    }

    fn key_scoreboard(&mut self, k: KeyEvent, gg: bool) {
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sel(-1),
            KeyCode::Char('g') if gg => self.sel = 0,
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') | KeyCode::End => {
                self.sel = self.games.as_ref().map_or(0, |g| g.len().saturating_sub(1))
            }
            KeyCode::Home => self.sel = 0,
            KeyCode::Char('h') | KeyCode::Left => self.change_date(-1),
            KeyCode::Char('l') | KeyCode::Right => self.change_date(1),
            KeyCode::Char(']') => self.find_game_day(true),
            KeyCode::Char('[') => self.find_game_day(false),
            KeyCode::Char('H') => self.change_date(-7),
            KeyCode::Char('L') => self.change_date(7),
            KeyCode::Char('t') => {
                self.date = baseball_today(self.cfg.day_rollover_hour) + Days::days(1);
                self.change_date(-1);
            }
            KeyCode::Char('r') => {
                self.fetch_schedule();
                self.flash("Refreshing scores");
            }
            KeyCode::Enter | KeyCode::Char('o') => {
                if let Some(pk) = self.selected_game().map(|g| g.game_pk) {
                    self.open_game(pk);
                }
            }
            _ => {}
        }
    }

    fn key_standings(&mut self, k: KeyEvent, gg: bool, ctrl: bool) {
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => self.screen = Screen::Scoreboard,
            KeyCode::Char('j') | KeyCode::Down => {
                self.standings_scroll = add(self.standings_scroll, 1)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.standings_scroll = add(self.standings_scroll, -1)
            }
            KeyCode::Char('d') if ctrl => self.standings_scroll = add(self.standings_scroll, 10),
            KeyCode::Char('u') if ctrl => self.standings_scroll = add(self.standings_scroll, -10),
            KeyCode::Char('g') if gg => self.standings_scroll = 0,
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => self.standings_scroll = u16::MAX / 2,
            KeyCode::Char('w') | KeyCode::Tab => {
                self.standings_kind = match self.standings_kind {
                    StandingsKind::Division => StandingsKind::WildCard,
                    StandingsKind::WildCard => StandingsKind::Division,
                };
                self.standings_scroll = 0;
                if !self.standings.contains_key(&self.standings_kind) {
                    self.fetch_standings();
                }
            }
            KeyCode::Char('r') => {
                self.fetch_standings();
                self.flash("Refreshing standings");
            }
            _ => {}
        }
    }

    fn key_game(&mut self, k: KeyEvent, gg: bool, ctrl: bool) {
        let Some(g) = self.game.as_mut() else {
            self.screen = Screen::Scoreboard;
            return;
        };
        let plays_len = g.plays().len();
        let mut flash: Option<String> = None;
        let mut refresh = false;
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                g.replay.playing = false;
                self.screen = match self.prev_screen {
                    Screen::Game => Screen::Scoreboard,
                    s => s,
                };
                return;
            }
            KeyCode::Tab => {
                let i = GameTab::ALL.iter().position(|t| *t == g.tab).unwrap_or(0);
                g.tab = GameTab::ALL[(i + 1) % GameTab::ALL.len()];
                g.scroll = 0;
            }
            KeyCode::BackTab => {
                let i = GameTab::ALL.iter().position(|t| *t == g.tab).unwrap_or(0);
                g.tab = GameTab::ALL[(i + GameTab::ALL.len() - 1) % GameTab::ALL.len()];
                g.scroll = 0;
            }
            KeyCode::Char(c) if GameTab::ALL.iter().any(|t| t.key() == c) => {
                g.tab = *GameTab::ALL.iter().find(|t| t.key() == c).unwrap();
                g.scroll = 0;
            }
            KeyCode::Char('t') => g.home_side = !g.home_side,
            KeyCode::Char('r') => {
                refresh = true;
                flash = Some("Refreshing game".into());
            }
            // Moving through the game.
            KeyCode::Char('h') | KeyCode::Left => {
                g.replay.playing = false;
                if let Some(c) = g.at().and_then(|c| c.prev(g.plays())) {
                    g.set_cursor(Some(c));
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                g.replay.playing = false;
                let next = g.at().and_then(|c| c.next(g.plays()));
                g.set_cursor(next.or(None));
            }
            KeyCode::Char('[') => {
                g.replay.playing = false;
                if let Some(c) = g.at().and_then(|c| c.prev_play(g.plays())) {
                    g.set_cursor(Some(c));
                }
            }
            KeyCode::Char(']') => {
                g.replay.playing = false;
                let next = g.at().and_then(|c| c.next_play(g.plays()));
                g.set_cursor(next);
            }
            KeyCode::Char('0') | KeyCode::Home if plays_len > 0 => {
                g.replay.playing = false;
                g.set_cursor(Some(Cursor::start()));
            }
            KeyCode::Char('$') | KeyCode::Char('f') | KeyCode::End => {
                g.replay.playing = false;
                g.cursor = None;
                flash = Some(if g.is_live() {
                    "Following live".into()
                } else {
                    "End of game".into()
                });
            }
            KeyCode::Char(' ') if plays_len > 0 => {
                if g.replay.playing {
                    g.replay.playing = false;
                    flash = Some("Replay paused".into());
                } else {
                    if g.following() {
                        g.cursor = Some(Cursor::start());
                    }
                    g.replay.playing = true;
                    g.replay.last_step = Instant::now();
                    flash = Some(format!("Replay {}x", SPEEDS[g.replay.speed_idx]));
                }
            }
            KeyCode::Char('R') if plays_len > 0 => {
                g.cursor = Some(Cursor::start());
                g.replay.playing = true;
                g.replay.last_step = Instant::now();
                flash = Some(format!(
                    "Replay from first pitch, {}x",
                    SPEEDS[g.replay.speed_idx]
                ));
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                g.replay.speed_idx = (g.replay.speed_idx + 1).min(SPEEDS.len() - 1);
                flash = Some(format!("Replay speed {}x", SPEEDS[g.replay.speed_idx]));
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                g.replay.speed_idx = g.replay.speed_idx.saturating_sub(1);
                flash = Some(format!("Replay speed {}x", SPEEDS[g.replay.speed_idx]));
            }
            // Scrolling.
            KeyCode::Char('j') | KeyCode::Down => {
                if g.tab == GameTab::Plays {
                    g.plays_sel = add_usize(g.plays_sel, 1, g.plays_len);
                } else {
                    g.scroll = add(g.scroll, 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if g.tab == GameTab::Plays {
                    g.plays_sel = add_usize(g.plays_sel, -1, g.plays_len);
                } else {
                    g.scroll = add(g.scroll, -1);
                }
            }
            KeyCode::Char('d') if ctrl => {
                if g.tab == GameTab::Plays {
                    g.plays_sel = add_usize(g.plays_sel, 10, g.plays_len);
                } else {
                    g.scroll = add(g.scroll, 10);
                }
            }
            KeyCode::Char('u') if ctrl => {
                if g.tab == GameTab::Plays {
                    g.plays_sel = add_usize(g.plays_sel, -10, g.plays_len);
                } else {
                    g.scroll = add(g.scroll, -10);
                }
            }
            KeyCode::Char('g') if gg => {
                g.scroll = 0;
                g.plays_sel = 0;
            }
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                g.scroll = u16::MAX / 2;
                g.plays_sel = g.plays_len.saturating_sub(1);
            }
            KeyCode::Char('<') | KeyCode::Char(',') => g.hscroll = g.hscroll.saturating_sub(8),
            KeyCode::Char('>') | KeyCode::Char('.') => g.hscroll = g.hscroll.saturating_add(8),
            KeyCode::Enter if g.tab == GameTab::Plays => {
                let log = g.log();
                if let Some(line) = log.get(g.plays_sel) {
                    let p = line.play;
                    let ev = g.plays()[p].play_events.len();
                    g.replay.playing = false;
                    g.set_cursor(Some(Cursor {
                        play: p,
                        events: ev,
                    }));
                    g.tab = GameTab::Gamecast;
                    g.scroll = 0;
                }
            }
            _ => {}
        }
        if let Some(f) = flash {
            self.flash(f);
        }
        if refresh {
            self.fetch_feed();
        }
    }
}

fn add(v: u16, d: i32) -> u16 {
    (v as i32 + d).clamp(0, u16::MAX as i32 / 2) as u16
}

fn add_usize(v: usize, d: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (v as i64 + d as i64).clamp(0, len as i64 - 1) as usize
}
