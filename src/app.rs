// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Application state, key handling and background fetching.

use crate::api::Api;
use crate::config::Config;
use crate::game::{
    BattedBall, Cursor, GamePitcher, LogLine, PitchRec, batted_balls, pitches, play_log,
};
use crate::model::{GameFeed, ScheduleGame, Standings, WpPoint};
use crate::stats::{ArsenalPitch, League, Load, PlayerStats, Stat, StatsCache};
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
    WinProb(u64, Result<Vec<WpPoint>>),
    Standings(StandingsKind, Result<Standings>),
    /// Nearest day with games, searching forward (`true`) or back.
    GameDay(bool, Result<Option<NaiveDate>>),
    Players(Vec<u64>, i32, Result<Vec<PlayerStats>>),
    Arsenal(u64, i32, Result<Vec<ArsenalPitch>>),
    HeadToHead((u64, u64), Result<Option<Stat>>),
    League(i32, Result<League>),
    Venue(u64, Result<Option<crate::model::FieldInfo>>),
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
    Matchup,
    Pitches,
    Spray,
    WinProb,
    Box,
    Scorecard,
    Plays,
}

impl GameTab {
    pub const ALL: [GameTab; 8] = [
        Self::Gamecast,
        Self::Matchup,
        Self::Pitches,
        Self::Spray,
        Self::WinProb,
        Self::Box,
        Self::Scorecard,
        Self::Plays,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Gamecast => "Gamecast",
            Self::Matchup => "Matchup",
            Self::Pitches => "Pitches",
            Self::Spray => "Spray",
            Self::WinProb => "Win prob",
            Self::Box => "Box score",
            Self::Scorecard => "Scorecard",
            Self::Plays => "Plays",
        }
    }
    pub fn key(self) -> char {
        match self {
            Self::Gamecast => 'c',
            Self::Matchup => 'm',
            Self::Pitches => 'i',
            Self::Spray => 'y',
            Self::WinProb => 'w',
            Self::Box => 'b',
            Self::Scorecard => 's',
            Self::Plays => 'p',
        }
    }
}

/// How far `[` / `]` look for a day with games.
pub const SEARCH_DAYS: i64 = 150;

/// Which team's balls in play the spray chart shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SprayTeam {
    Both,
    Away,
    Home,
}

impl SprayTeam {
    fn next(self) -> Self {
        match self {
            Self::Both => Self::Away,
            Self::Away => Self::Home,
            Self::Home => Self::Both,
        }
    }
}

/// Biggest swings listed on the win-probability tab.
pub const SWINGS_SHOWN: usize = 8;

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
    pub spray_sel: usize,
    pub spray_team: SprayTeam,
    /// Only the batter at the shown moment.
    pub spray_batter_only: bool,
    /// Pitcher picked on the pitches tab; `None` follows whoever is pitching.
    pub pitch_sel: Option<u64>,
    /// Win probability per plate appearance, refreshed as plays finish.
    pub wp: Option<Vec<WpPoint>>,
    pub wp_loading: bool,
    pub wp_error: Option<String>,
    /// Finished plays when win probability was last fetched.
    pub wp_plays: usize,
    pub wp_fetched: Option<Instant>,
    pub wp_sel: usize,
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
            spray_sel: usize::MAX,
            spray_team: SprayTeam::Both,
            spray_batter_only: false,
            pitch_sel: None,
            wp: None,
            wp_loading: false,
            wp_error: None,
            wp_plays: 0,
            wp_fetched: None,
            wp_sel: 0,
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

    /// Balls in play shown on the spray chart, after the filters.
    pub fn spray_balls(&self) -> Vec<BattedBall> {
        let Some(c) = self.at() else {
            return Vec::new();
        };
        let plays = self.plays();
        let batter = plays.get(c.play).map(|p| p.matchup.batter.id);
        batted_balls(plays, c)
            .into_iter()
            .filter(|b| match self.spray_team {
                SprayTeam::Both => true,
                SprayTeam::Away => b.top,
                SprayTeam::Home => !b.top,
            })
            .filter(|b| !self.spray_batter_only || Some(b.batter) == batter)
            .collect()
    }

    /// The pitcher at the shown moment.
    pub fn current_pitcher(&self) -> Option<u64> {
        let c = self.at()?;
        self.plays().get(c.play).map(|p| p.matchup.pitcher.id)
    }

    /// The pitcher shown on the pitches tab.
    pub fn shown_pitcher(&self) -> Option<u64> {
        self.pitch_sel.or_else(|| self.current_pitcher())
    }

    /// Pitches and pitchers up to the shown moment.
    pub fn pitch_data(&self) -> (Vec<PitchRec>, Vec<GamePitcher>) {
        match self.at() {
            Some(c) => pitches(self.plays(), c),
            None => (Vec::new(), Vec::new()),
        }
    }

    /// Steps the pitches tab's pick through the pitchers, away team first.
    fn move_pitcher(&mut self, d: i32) {
        let (_, mut list) = self.pitch_data();
        list.sort_by_key(|p| p.home);
        if list.is_empty() {
            return;
        }
        let cur = self.shown_pitcher();
        let i = list.iter().position(|p| Some(p.id) == cur).unwrap_or(0);
        let j = add_usize(i, d, list.len());
        self.pitch_sel = Some(list[j].id);
    }

    /// Win probability at the shown moment: after the last finished play.
    pub fn wp_now(&self) -> Option<&WpPoint> {
        let c = self.at()?;
        let plays = self.plays();
        let done = |i: usize| {
            plays.get(i).is_some_and(|p| {
                p.about.is_complete && (i < c.play || c.events >= p.play_events.len())
            })
        };
        let last = (0..=c.play).rev().find(|i| done(*i))?;
        self.wp.as_ref()?.iter().find(|w| w.at_bat_index == last)
    }

    /// Plays up to the shown moment, biggest win-probability swings first.
    pub fn wp_swings(&self) -> Vec<WpPoint> {
        let Some(c) = self.at() else {
            return Vec::new();
        };
        let plays = self.plays();
        let mut v: Vec<WpPoint> = self
            .wp
            .as_ref()
            .map(|w| {
                w.iter()
                    .filter(|p| {
                        p.at_bat_index < c.play
                            || (p.at_bat_index == c.play
                                && plays
                                    .get(c.play)
                                    .is_some_and(|pl| c.events >= pl.play_events.len()))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| {
            b.home_team_win_probability_added
                .abs()
                .total_cmp(&a.home_team_win_probability_added.abs())
        });
        v
    }

    /// The selectable list on the current tab, if it has one.
    fn list_sel(&mut self) -> Option<(&mut usize, usize)> {
        match self.tab {
            GameTab::Plays => Some((&mut self.plays_sel, self.plays_len)),
            GameTab::WinProb => {
                let len = self.wp_swings().len().min(SWINGS_SHOWN);
                Some((&mut self.wp_sel, len))
            }
            GameTab::Spray => {
                let len = self.spray_balls().len();
                // usize::MAX means "newest", so new balls stay selected live.
                if self.spray_sel >= len {
                    self.spray_sel = len.saturating_sub(1);
                }
                Some((&mut self.spray_sel, len))
            }
            _ => None,
        }
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
    pub stats: StatsCache,
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
            stats: StatsCache::default(),
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
                Msg::WinProb(pk, r) => {
                    let Some(g) = self.game.as_mut() else {
                        continue;
                    };
                    if g.pk != pk {
                        continue;
                    }
                    g.wp_loading = false;
                    g.wp_fetched = Some(Instant::now());
                    match r {
                        Ok(w) => {
                            g.wp_plays = w.len();
                            g.wp = Some(w);
                            g.wp_error = None;
                        }
                        Err(e) => g.wp_error = Some(format!("{e:#}")),
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
                Msg::Players(ids, season, r) => match r {
                    Ok(list) => {
                        for id in &ids {
                            self.stats.players.remove(&(*id, season));
                        }
                        for p in list {
                            self.stats.players.insert((p.id, season), Load::Ready(p));
                        }
                        // Players the API returned nothing for.
                        for id in ids {
                            self.stats
                                .players
                                .entry((id, season))
                                .or_insert_with(|| Load::Failed("no stats".into()));
                        }
                    }
                    Err(e) => {
                        for id in ids {
                            self.stats
                                .players
                                .insert((id, season), Load::Failed(format!("{e:#}")));
                        }
                    }
                },
                Msg::Arsenal(id, season, r) => {
                    self.stats.arsenal.insert(
                        (id, season),
                        r.map(Load::Ready)
                            .unwrap_or_else(|e| Load::Failed(format!("{e:#}"))),
                    );
                }
                Msg::HeadToHead(key, r) => {
                    self.stats.head_to_head.insert(
                        key,
                        r.map(Load::Ready)
                            .unwrap_or_else(|e| Load::Failed(format!("{e:#}"))),
                    );
                }
                Msg::League(season, r) => {
                    self.stats.league.insert(
                        season,
                        r.map(Load::Ready)
                            .unwrap_or_else(|e| Load::Failed(format!("{e:#}"))),
                    );
                }
                Msg::Venue(id, r) => {
                    self.stats.venues.insert(
                        id,
                        r.map(Load::Ready)
                            .unwrap_or_else(|e| Load::Failed(format!("{e:#}"))),
                    );
                }
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
        self.ensure_win_prob();
        self.ensure_matchup_stats();
    }

    /// Fetches win probability once a game has plays, and again whenever
    /// more plays have finished (at most every live poll).
    fn ensure_win_prob(&mut self) {
        let poll = Duration::from_secs(self.cfg.live_poll_seconds);
        let Some(g) = self.game.as_mut() else { return };
        let Some(feed) = g.feed.as_ref() else { return };
        if g.wp_loading {
            return;
        }
        let done = feed
            .live_data
            .plays
            .all_plays
            .iter()
            .filter(|p| p.about.is_complete)
            .count();
        let stale = g.wp.is_none() || done > g.wp_plays;
        let due = g.wp_fetched.is_none_or(|t| t.elapsed() >= poll);
        if done == 0 || !stale || !due {
            return;
        }
        g.wp_loading = true;
        let (tx, api, pk) = (self.tx.clone(), self.api.clone(), g.pk);
        thread::spawn(move || {
            let _ = tx.send(Msg::WinProb(pk, api.win_probability(pk)));
        });
    }

    /// Fetches stats for the batter and pitcher at the shown moment, once
    /// each. Used by the matchup tab and the gamecast's stat lines.
    fn ensure_matchup_stats(&mut self) {
        if self.screen != Screen::Game {
            return;
        }
        let Some(g) = self.game.as_ref() else { return };
        let Some(feed) = g.feed.as_ref() else { return };
        let Some(c) = g.at() else { return };
        let Some(play) = feed.live_data.plays.all_plays.get(c.play) else {
            return;
        };
        let season: i32 = feed.game_data.game.season.parse().unwrap_or(0);
        if season == 0 {
            return;
        }
        let venue = feed.game_data.venue.id;
        if venue != 0 && !self.stats.venues.contains_key(&venue) {
            self.stats.venues.insert(venue, Load::Loading);
            let (tx, api) = (self.tx.clone(), self.api.clone());
            thread::spawn(move || {
                let _ = tx.send(Msg::Venue(venue, api.venue(venue)));
            });
        }
        let (b, p) = (play.matchup.batter.id, play.matchup.pitcher.id);
        let missing: Vec<u64> = [b, p]
            .into_iter()
            .filter(|id| *id != 0 && !self.stats.players.contains_key(&(*id, season)))
            .collect();
        if !missing.is_empty() {
            for id in &missing {
                self.stats.players.insert((*id, season), Load::Loading);
            }
            let (tx, api) = (self.tx.clone(), self.api.clone());
            thread::spawn(move || {
                let r = api.player_stats(&missing, season);
                let _ = tx.send(Msg::Players(missing, season, r));
            });
        }
        let mut arsenal_for = vec![p];
        if g.tab == GameTab::Pitches
            && let Some(sp) = g.shown_pitcher()
            && sp != p
        {
            arsenal_for.push(sp);
        }
        for p in arsenal_for {
            if p == 0 || self.stats.arsenal.contains_key(&(p, season)) {
                continue;
            }
            self.stats.arsenal.insert((p, season), Load::Loading);
            let (tx, api) = (self.tx.clone(), self.api.clone());
            thread::spawn(move || {
                let _ = tx.send(Msg::Arsenal(p, season, api.arsenal(p, season)));
            });
        }
        if b != 0 && p != 0 && !self.stats.head_to_head.contains_key(&(b, p)) {
            self.stats.head_to_head.insert((b, p), Load::Loading);
            let (tx, api) = (self.tx.clone(), self.api.clone());
            thread::spawn(move || {
                let _ = tx.send(Msg::HeadToHead((b, p), api.head_to_head(b, p)));
            });
        }
        if let std::collections::hash_map::Entry::Vacant(e) = self.stats.league.entry(season) {
            e.insert(Load::Loading);
            let (tx, api) = (self.tx.clone(), self.api.clone());
            thread::spawn(move || {
                let _ = tx.send(Msg::League(season, api.league(season)));
            });
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
                    if let Some((sel, len)) = g.list_sel() {
                        *sel = add_usize(*sel, delta.signum(), len);
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
            KeyCode::Char('t') if g.tab == GameTab::Spray => {
                g.spray_team = g.spray_team.next();
                g.spray_sel = usize::MAX;
            }
            KeyCode::Char('a') if g.tab == GameTab::Spray => {
                g.spray_batter_only = !g.spray_batter_only;
                g.spray_sel = usize::MAX;
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
            // Pitches tab: pick a pitcher, `a` back to whoever is pitching.
            KeyCode::Char('j') | KeyCode::Down if g.tab == GameTab::Pitches => g.move_pitcher(1),
            KeyCode::Char('k') | KeyCode::Up if g.tab == GameTab::Pitches => g.move_pitcher(-1),
            KeyCode::Char('a') if g.tab == GameTab::Pitches => {
                g.pitch_sel = None;
                flash = Some("Following the current pitcher".into());
            }
            // Scrolling.
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some((sel, len)) = g.list_sel() {
                    *sel = add_usize(*sel, 1, len);
                } else {
                    g.scroll = add(g.scroll, 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some((sel, len)) = g.list_sel() {
                    *sel = add_usize(*sel, -1, len);
                } else {
                    g.scroll = add(g.scroll, -1);
                }
            }
            KeyCode::Char('d') if ctrl => {
                if let Some((sel, len)) = g.list_sel() {
                    *sel = add_usize(*sel, 10, len);
                } else {
                    g.scroll = add(g.scroll, 10);
                }
            }
            KeyCode::Char('u') if ctrl => {
                if let Some((sel, len)) = g.list_sel() {
                    *sel = add_usize(*sel, -10, len);
                } else {
                    g.scroll = add(g.scroll, -10);
                }
            }
            KeyCode::Char('g') if gg => {
                g.scroll = 0;
                g.plays_sel = 0;
                g.spray_sel = 0;
            }
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                g.scroll = u16::MAX / 2;
                g.plays_sel = g.plays_len.saturating_sub(1);
                g.spray_sel = usize::MAX;
            }
            KeyCode::Char('<') | KeyCode::Char(',') => g.hscroll = g.hscroll.saturating_sub(8),
            KeyCode::Char('>') | KeyCode::Char('.') => g.hscroll = g.hscroll.saturating_add(8),
            KeyCode::Enter if g.tab == GameTab::WinProb => {
                let swings = g.wp_swings();
                if let Some(w) = swings.get(g.wp_sel) {
                    let p = w.at_bat_index;
                    if let Some(pl) = g.plays().get(p) {
                        let ev = pl.play_events.len();
                        g.replay.playing = false;
                        g.set_cursor(Some(Cursor {
                            play: p,
                            events: ev,
                        }));
                        g.tab = GameTab::Gamecast;
                        g.scroll = 0;
                    }
                }
            }
            KeyCode::Enter if g.tab == GameTab::Spray => {
                let balls = g.spray_balls();
                if let Some(b) = balls.get(g.spray_sel.min(balls.len().saturating_sub(1))) {
                    let p = b.play;
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
            // Let stats that failed to load try again.
            self.stats.retry_failed();
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
