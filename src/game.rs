// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Rebuilds the state of a game at any point from its play list: score,
//! bases, outs, count, linescore, per-player tallies and scorecard cells.
//! The live view is simply this state at the last event.

use crate::model::{GameFeed, Play, PlayEvent, Runner};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A point in the game: `events` of the play's events have happened.
/// `events == 0` is the moment the batter steps in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub play: usize,
    pub events: usize,
}

impl Cursor {
    /// The end of the game so far.
    pub fn end(plays: &[Play]) -> Option<Self> {
        let last = plays.len().checked_sub(1)?;
        Some(Self {
            play: last,
            events: plays[last].play_events.len(),
        })
    }

    pub fn start() -> Self {
        Self { play: 0, events: 0 }
    }

    /// Next event (or the next batter stepping in).
    pub fn next(self, plays: &[Play]) -> Option<Self> {
        let len = plays.get(self.play)?.play_events.len();
        if self.events < len {
            Some(Self {
                events: self.events + 1,
                ..self
            })
        } else if self.play + 1 < plays.len() {
            Some(Self {
                play: self.play + 1,
                events: 0,
            })
        } else {
            None
        }
    }

    pub fn prev(self, plays: &[Play]) -> Option<Self> {
        if self.events > 0 {
            Some(Self {
                events: self.events - 1,
                ..self
            })
        } else if self.play > 0 {
            let p = self.play - 1;
            Some(Self {
                play: p,
                events: plays[p].play_events.len(),
            })
        } else {
            None
        }
    }

    /// End of the previous plate appearance.
    pub fn prev_play(self, plays: &[Play]) -> Option<Self> {
        let target = self.play.checked_sub(1)?;
        Some(Self {
            play: target,
            events: plays[target].play_events.len(),
        })
    }

    /// End of the next plate appearance (or of this one, if mid at-bat).
    pub fn next_play(self, plays: &[Play]) -> Option<Self> {
        let len = plays.get(self.play)?.play_events.len();
        if self.events < len {
            return Some(Self {
                events: len,
                ..self
            });
        }
        let p = self.play + 1;
        plays.get(p).map(|pl| Self {
            play: p,
            events: pl.play_events.len(),
        })
    }

    /// Ordinal position for "step x of y" displays.
    pub fn ordinal(self, plays: &[Play]) -> usize {
        plays[..self.play.min(plays.len())]
            .iter()
            .map(|p| p.play_events.len() + 1)
            .sum::<usize>()
            + self.events
    }
}

pub fn total_steps(plays: &[Play]) -> usize {
    plays
        .iter()
        .map(|p| p.play_events.len() + 1)
        .sum::<usize>()
        .saturating_sub(1)
}

/// Team index: 0 = away (bats in the top half), 1 = home.
pub fn batting_team(play: &Play) -> usize {
    if play.about.is_top_inning { 0 } else { 1 }
}

pub const HIT_EVENTS: [&str; 4] = ["single", "double", "triple", "home_run"];

#[derive(Debug, Clone, Default)]
pub struct PitcherTally {
    pub pitches: u32,
    pub strikes: u32,
    pub strikeouts: u32,
    pub walks: u32,
    pub hits: u32,
    pub outs: u32,
    pub runs: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Situation {
    pub inning: u32,
    pub top: bool,
    pub outs: u32,
    pub balls: u32,
    pub strikes: u32,
    /// Runner ids on first, second, third.
    pub bases: [Option<u64>; 3],
    pub runs: [u32; 2],
    pub hits: [u32; 2],
    pub errors: [u32; 2],
    /// Runs per inning, `[away, home]`; `None` = that half hasn't started.
    pub line: Vec<[Option<u32>; 2]>,
    /// Plate-appearance results so far, per batter.
    pub batter_results: HashMap<u64, Vec<String>>,
    pub pitchers: HashMap<u64, PitcherTally>,
    /// The current play has finished (its result is known).
    pub play_done: bool,
}

/// Runner segments of a play that have happened once `events` events are
/// shown, in the order they happened.
pub fn runner_segments(play: &Play, events: usize, whole: bool) -> Vec<&Runner> {
    let mut segs: Vec<(usize, usize, &Runner)> = play
        .runners
        .iter()
        .enumerate()
        .filter(|(_, r)| match r.details.play_index {
            Some(i) => whole || i < events,
            None => whole,
        })
        .map(|(n, r)| (r.details.play_index.unwrap_or(usize::MAX), n, r))
        .collect();
    segs.sort_by_key(|(i, n, _)| (*i, *n));
    segs.into_iter().map(|(_, _, r)| r).collect()
}

fn base_index(b: &str) -> Option<usize> {
    match b {
        "1B" => Some(0),
        "2B" => Some(1),
        "3B" => Some(2),
        _ => None,
    }
}

impl Situation {
    pub fn at(plays: &[Play], cursor: Cursor) -> Self {
        let mut s = Situation::default();
        let mut half: Option<(u32, bool)> = None;
        for (i, play) in plays.iter().enumerate().take(cursor.play + 1) {
            let shown = if i < cursor.play {
                play.play_events.len()
            } else {
                cursor.events.min(play.play_events.len())
            };
            let whole = shown == play.play_events.len() && play.about.is_complete;
            let key = (play.about.inning, play.about.is_top_inning);
            if half != Some(key) {
                half = Some(key);
                s.bases = [None; 3];
                s.outs = 0;
            }
            s.inning = play.about.inning;
            s.top = play.about.is_top_inning;
            let team = batting_team(play);
            let inn = play.about.inning.max(1) as usize;
            if s.line.len() < inn {
                s.line.resize(inn, [None, None]);
            }
            s.line[inn - 1][team].get_or_insert(0);

            let pitcher = play.matchup.pitcher.id;
            let mut action_errors = 0;
            for ev in &play.play_events[..shown] {
                if let (Some(new), Some(old)) = (&ev.player, &ev.replaced_player) {
                    for b in s.bases.iter_mut() {
                        if *b == Some(old.id) {
                            *b = Some(new.id);
                        }
                    }
                }
                // Errors that move no runner (a dropped foul pop) only show
                // up as an action.
                if ev.kind == "action" && ev.details.event_type.as_deref() == Some("error") {
                    action_errors += 1;
                }
                if ev.is_pitch {
                    let t = s.pitchers.entry(pitcher).or_default();
                    t.pitches += 1;
                    if !ev.details.is_ball {
                        t.strikes += 1;
                    }
                }
            }

            let mut error_credits = HashSet::new();
            for r in runner_segments(play, shown, whole) {
                if let Some(rid) = Some(r.details.runner.id) {
                    for b in s.bases.iter_mut() {
                        if *b == Some(rid) {
                            *b = None;
                        }
                    }
                }
                if r.movement.is_out == Some(true) {
                    s.outs += 1;
                    s.pitchers.entry(pitcher).or_default().outs += 1;
                } else if let Some(end) = r.movement.end.as_deref() {
                    if end == "score" {
                        s.runs[team] += 1;
                        *s.line[inn - 1][team].get_or_insert(0) += 1;
                        s.pitchers.entry(pitcher).or_default().runs += 1;
                    } else if let Some(bi) = base_index(end) {
                        s.bases[bi] = Some(r.details.runner.id);
                    }
                }
                for c in &r.credits {
                    if c.credit.contains("error") {
                        error_credits.insert((c.player.id, c.credit.clone()));
                    }
                }
            }
            s.errors[1 - team] += error_credits.len() as u32 + action_errors;

            if whole {
                let et = play.result.event_type.as_deref().unwrap_or("");
                if HIT_EVENTS.contains(&et) {
                    s.hits[team] += 1;
                    s.pitchers.entry(pitcher).or_default().hits += 1;
                }
                if et.starts_with("strikeout") {
                    s.pitchers.entry(pitcher).or_default().strikeouts += 1;
                }
                if et == "walk" || et == "intent_walk" {
                    s.pitchers.entry(pitcher).or_default().walks += 1;
                }
                if play.result.kind == "atBat" {
                    s.batter_results
                        .entry(play.matchup.batter.id)
                        .or_default()
                        .push(short_result(play));
                }
            }

            if i == cursor.play {
                s.play_done = whole;
                if shown > 0 {
                    let c = play.play_events[shown - 1].count;
                    s.balls = c.balls;
                    s.strikes = c.strikes;
                } else {
                    s.balls = 0;
                    s.strikes = 0;
                }
            }
        }
        s
    }
}

/// Short result for "1-for-3, K, HR" style batter lines.
pub fn short_result(play: &Play) -> String {
    let et = play.result.event_type.as_deref().unwrap_or("");
    match et {
        "single" => "1B".into(),
        "double" => "2B".into(),
        "triple" => "3B".into(),
        "home_run" => "HR".into(),
        "walk" => "BB".into(),
        "intent_walk" => "IBB".into(),
        "hit_by_pitch" => "HBP".into(),
        e if e.starts_with("strikeout") => "K".into(),
        "sac_fly" | "sac_fly_double_play" => "SF".into(),
        "sac_bunt" | "sac_bunt_double_play" => "SAC".into(),
        "field_error" => "E".into(),
        "fielders_choice" | "fielders_choice_out" | "force_out" => "FC".into(),
        "grounded_into_double_play" | "double_play" => "DP".into(),
        "triple_play" => "TP".into(),
        "catcher_interf" => "CI".into(),
        "field_out" => match trajectory(play).as_deref() {
            Some("fly_ball") => "FO".into(),
            Some("popup") => "PO".into(),
            Some("line_drive") => "LO".into(),
            _ => "GO".into(),
        },
        _ => play
            .result
            .event
            .clone()
            .unwrap_or_default()
            .chars()
            .take(3)
            .collect(),
    }
}

/// "1-for-3" from a batter's results, counting only official at-bats.
pub fn hits_for(results: &[String]) -> (u32, u32) {
    let mut ab = 0;
    let mut h = 0;
    for r in results {
        match r.as_str() {
            "BB" | "IBB" | "HBP" | "SF" | "SAC" | "CI" => {}
            "1B" | "2B" | "3B" | "HR" => {
                ab += 1;
                h += 1;
            }
            _ => ab += 1,
        }
    }
    (h, ab)
}

pub fn trajectory(play: &Play) -> Option<String> {
    play.play_events
        .iter()
        .rev()
        .find_map(|e| e.hit_data.as_ref()?.trajectory.clone())
}

fn last_pitch(play: &Play) -> Option<&PlayEvent> {
    play.play_events.iter().rev().find(|e| e.is_pitch)
}

/// Fielders involved in the outs of a play, in order, e.g. `6-4-3`.
fn out_sequence(play: &Play) -> Vec<String> {
    let mut outs: Vec<&Runner> = play
        .runners
        .iter()
        .filter(|r| r.movement.is_out == Some(true))
        .collect();
    outs.sort_by_key(|r| r.movement.out_number.unwrap_or(0));
    let mut seq: Vec<String> = Vec::new();
    for r in outs {
        for c in &r.credits {
            if (c.credit == "f_assist" || c.credit == "f_putout" || c.credit == "f_assist_of")
                && seq.last() != Some(&c.position.code)
            {
                seq.push(c.position.code.clone());
            }
        }
    }
    seq
}

fn error_position(play: &Play) -> Option<String> {
    play.runners
        .iter()
        .flat_map(|r| r.credits.iter())
        .find(|c| c.credit.contains("error"))
        .map(|c| c.position.code.clone())
}

/// Traditional scorebook notation for a plate appearance.
pub fn notation(play: &Play) -> String {
    let et = play.result.event_type.as_deref().unwrap_or("");
    let seq = out_sequence(play);
    let joined = seq.join("-");
    match et {
        "single" => "1B".into(),
        "double" => "2B".into(),
        "triple" => "3B".into(),
        "home_run" => "HR".into(),
        "walk" => "BB".into(),
        "intent_walk" => "IBB".into(),
        "hit_by_pitch" => "HBP".into(),
        "catcher_interf" => "CI".into(),
        "strikeout" | "strikeout_double_play" | "strikeout_triple_play" => {
            let looking = last_pitch(play)
                .and_then(|e| e.details.call.as_ref())
                .is_some_and(|c| c.code == "C");
            let k = if looking { "KL" } else { "K" };
            match et {
                "strikeout_double_play" => format!("{k} DP"),
                "strikeout_triple_play" => format!("{k} TP"),
                _ => k.into(),
            }
        }
        "field_out" => {
            if seq.len() > 1 {
                joined
            } else if let Some(p) = seq.first() {
                match trajectory(play).as_deref() {
                    Some("fly_ball") => format!("F{p}"),
                    Some("popup") => format!("P{p}"),
                    Some("line_drive") => format!("L{p}"),
                    _ => format!("{p}U"),
                }
            } else {
                "OUT".into()
            }
        }
        "grounded_into_double_play" | "double_play" => format!("{joined} DP"),
        "triple_play" => format!("{joined} TP"),
        "force_out" | "fielders_choice_out" => {
            if joined.is_empty() {
                "FC".into()
            } else {
                format!("FC {joined}")
            }
        }
        "fielders_choice" => "FC".into(),
        "field_error" => format!("E{}", error_position(play).unwrap_or_default()),
        "sac_fly" => format!("SF{}", seq.last().cloned().unwrap_or_default()),
        "sac_fly_double_play" => format!("SF {joined} DP"),
        "sac_bunt" => format!("SH {joined}"),
        "sac_bunt_double_play" => format!("SH {joined} DP"),
        _ => play
            .result
            .event
            .clone()
            .unwrap_or_default()
            .split_whitespace()
            .map(|w| w.chars().next().unwrap_or(' '))
            .collect(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Hit,
    Walk,
    Strikeout,
    Out,
    Reached,
}

/// One plate appearance on the scorecard.
#[derive(Debug, Clone)]
pub struct PaCell {
    pub notation: String,
    pub kind: CellKind,
    /// Furthest base reached: 0 none, 1-3 bases, 4 scored.
    pub reached: u8,
    /// Base the runner was put out at, after reaching.
    pub out_at: Option<u8>,
    /// Out number when the batter himself made the out.
    pub out_number: Option<u32>,
    pub rbi: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SlotRow {
    /// Players who batted in this lineup spot: (id, name, position).
    pub players: Vec<(u64, String, String)>,
    pub cells: BTreeMap<u32, Vec<PaCell>>,
}

#[derive(Debug, Clone, Default)]
pub struct Scorecard {
    pub rows: Vec<SlotRow>,
    pub innings: u32,
    /// Runs and hits per inning.
    pub totals: BTreeMap<u32, (u32, u32)>,
}

fn cell_kind(et: &str) -> CellKind {
    match et {
        e if HIT_EVENTS.contains(&e) => CellKind::Hit,
        "walk" | "intent_walk" | "hit_by_pitch" | "catcher_interf" => CellKind::Walk,
        e if e.starts_with("strikeout") => CellKind::Strikeout,
        "field_error" | "fielders_choice" | "force_out" | "fielders_choice_out" => {
            CellKind::Reached
        }
        _ => CellKind::Out,
    }
}

fn base_num(b: &str) -> u8 {
    match b {
        "1B" => 1,
        "2B" => 2,
        "3B" => 3,
        "score" | "4B" => 4,
        _ => 0,
    }
}

/// Builds one team's scorecard from the plays up to `cursor`.
pub fn scorecard(feed: &GameFeed, home: bool, cursor: Option<Cursor>) -> Scorecard {
    let plays = &feed.live_data.plays.all_plays;
    let team = feed.live_data.boxscore.teams.get(home);
    let mut card = Scorecard {
        rows: vec![SlotRow::default(); 9],
        innings: feed
            .live_data
            .linescore
            .scheduled_innings
            .unwrap_or(9)
            .max(plays.last().map(|p| p.about.inning).unwrap_or(0)),
        ..Default::default()
    };

    // Lineup spots, starters first, then substitutes in the order they entered.
    let mut order: Vec<(usize, usize, u64)> = Vec::new();
    for id in &team.batters {
        if let Some(p) = team.player(*id)
            && let Some((slot, _)) = p.lineup_slot()
        {
            let n: usize = p
                .batting_order
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            order.push((slot, n, *id));
        }
    }
    order.sort();
    for (slot, _, id) in order {
        if (1..=9).contains(&slot) {
            let p = team.player(id).expect("listed player");
            let name = feed
                .game_data
                .players
                .get(&format!("ID{id}"))
                .map(|i| i.boxscore_name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| p.person.full_name.clone());
            let pos = p
                .all_positions
                .first()
                .map(|x| x.abbreviation.clone())
                .unwrap_or_else(|| p.position.abbreviation.clone());
            card.rows[slot - 1].players.push((id, name, pos));
        }
    }
    let slot_of: HashMap<u64, usize> = card
        .rows
        .iter()
        .enumerate()
        .flat_map(|(i, r)| r.players.iter().map(move |(id, _, _)| (*id, i)))
        .collect();

    let scheduled = feed.live_data.linescore.scheduled_innings.unwrap_or(9);
    let end = cursor.or_else(|| Cursor::end(plays));
    let Some(end) = end else { return card };
    // runner id -> (slot, inning, index in that cell list)
    let mut on_base: HashMap<u64, (usize, u32, usize)> = HashMap::new();
    let mut half: Option<(u32, bool)> = None;
    let mut auto_runner_seen = false;
    for (i, play) in plays.iter().enumerate().take(end.play + 1) {
        if play.about.is_top_inning == home {
            continue;
        }
        let shown = if i < end.play {
            play.play_events.len()
        } else {
            end.events.min(play.play_events.len())
        };
        let whole = shown == play.play_events.len() && play.about.is_complete;
        let key = (play.about.inning, play.about.is_top_inning);
        if half != Some(key) {
            half = Some(key);
            on_base.clear();
            auto_runner_seen = false;
        }
        let inning = play.about.inning;
        let batter = play.matchup.batter.id;
        let totals = card.totals.entry(inning).or_default();

        for ev in &play.play_events[..shown] {
            if let (Some(new), Some(old)) = (&ev.player, &ev.replaced_player)
                && let Some(v) = on_base.remove(&old.id)
            {
                on_base.insert(new.id, v);
            }
        }
        if whole && play.result.kind == "atBat" {
            let et = play.result.event_type.as_deref().unwrap_or("");
            if HIT_EVENTS.contains(&et) {
                totals.1 += 1;
            }
            if let Some(&slot) = slot_of.get(&batter) {
                let out_number = play
                    .runners
                    .iter()
                    .find(|r| r.details.runner.id == batter && r.movement.is_out == Some(true))
                    .and_then(|r| r.movement.out_number);
                let cells = card.rows[slot].cells.entry(inning).or_default();
                cells.push(PaCell {
                    notation: notation(play),
                    kind: cell_kind(et),
                    reached: 0,
                    out_at: None,
                    out_number,
                    rbi: play.result.rbi,
                });
                on_base.insert(batter, (slot, inning, cells.len() - 1));
            }
        }
        for r in runner_segments(play, shown, whole) {
            let rid = r.details.runner.id;
            if !on_base.contains_key(&rid)
                && !auto_runner_seen
                && inning > scheduled
                && r.movement.start.is_some()
            {
                auto_runner_seen = true;
                // Extra-inning automatic runner: starts on second without
                // batting, scored as "AR" in his lineup spot.
                if let Some(&slot) = slot_of.get(&rid) {
                    let cells = card.rows[slot].cells.entry(inning).or_default();
                    cells.push(PaCell {
                        notation: "AR".into(),
                        kind: CellKind::Reached,
                        reached: 2,
                        out_at: None,
                        out_number: None,
                        rbi: 0,
                    });
                    on_base.insert(rid, (slot, inning, cells.len() - 1));
                }
            }
            let Some(&(slot, inn, idx)) = on_base.get(&rid) else {
                if r.movement.end.as_deref() == Some("score") {
                    card.totals.entry(inning).or_default().0 += 1;
                }
                continue;
            };
            let Some(cell) = card.rows[slot]
                .cells
                .get_mut(&inn)
                .and_then(|c| c.get_mut(idx))
            else {
                continue;
            };
            if r.movement.is_out == Some(true) {
                if cell.out_number.is_none() {
                    cell.out_at = r.movement.out_base.as_deref().map(base_num);
                }
                on_base.remove(&rid);
            } else if let Some(end) = r.movement.end.as_deref() {
                cell.reached = cell.reached.max(base_num(end));
                if end == "score" {
                    card.totals.entry(inning).or_default().0 += 1;
                    on_base.remove(&rid);
                }
            }
        }
    }
    card
}

/// Lines for the play log: one per finished plate appearance or notable
/// in-play event, newest last.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub play: usize,
    pub inning: u32,
    pub top: bool,
    pub outs: u32,
    pub text: String,
    pub scoring: bool,
    pub score: Option<(u32, u32)>,
    pub is_action: bool,
}

pub fn play_log(plays: &[Play], cursor: Cursor) -> Vec<LogLine> {
    let mut out = Vec::new();
    let mut outs = 0;
    let mut half = None;
    let mut runs = [0u32; 2];
    for (i, play) in plays.iter().enumerate().take(cursor.play + 1) {
        let shown = if i < cursor.play {
            play.play_events.len()
        } else {
            cursor.events.min(play.play_events.len())
        };
        let whole = shown == play.play_events.len() && play.about.is_complete;
        let key = (play.about.inning, play.about.is_top_inning);
        if half != Some(key) {
            half = Some(key);
            outs = 0;
        }
        let team = batting_team(play);
        // In-play actions worth a line of their own (steals, wild pitches,
        // substitutions), shown once they've happened.
        for ev in &play.play_events[..shown] {
            if ev.kind == "action" {
                let et = ev.details.event_type.as_deref().unwrap_or("");
                if matches!(et, "batter_timeout" | "mound_visit" | "game_advisory") {
                    continue;
                }
                if let Some(d) = ev.details.description.as_deref() {
                    out.push(LogLine {
                        play: i,
                        inning: play.about.inning,
                        top: play.about.is_top_inning,
                        outs,
                        text: d.trim().to_string(),
                        scoring: ev.details.is_scoring_play == Some(true),
                        score: None,
                        is_action: true,
                    });
                }
            }
        }
        let mut scored = false;
        for r in runner_segments(play, shown, whole) {
            if r.movement.is_out == Some(true) {
                outs += 1;
            } else if r.movement.end.as_deref() == Some("score") {
                runs[team] += 1;
                scored = true;
            }
        }
        if whole {
            out.push(LogLine {
                play: i,
                inning: play.about.inning,
                top: play.about.is_top_inning,
                outs,
                text: play
                    .result
                    .description
                    .clone()
                    .unwrap_or_else(|| play.result.event.clone().unwrap_or_default()),
                scoring: scored,
                score: scored.then_some((runs[0], runs[1])),
                is_action: false,
            });
        }
    }
    out
}

/// A ball put in play, for the spray chart.
#[derive(Debug, Clone)]
pub struct BattedBall {
    pub play: usize,
    pub inning: u32,
    pub top: bool,
    pub batter: u64,
    pub batter_name: String,
    pub event_type: String,
    /// Scorebook notation of the result (`1B`, `F8`, `6-3`…).
    pub result: String,
    pub launch_speed: Option<f64>,
    pub launch_angle: Option<f64>,
    pub distance: Option<f64>,
    pub trajectory: Option<String>,
    /// Feet from home plate: x toward right field, y toward center.
    pub x: f64,
    pub y: f64,
}

/// Where Gameday's batted-ball coordinates put home plate, and feet per
/// unit (fitted to home-run distances).
const PLATE: (f64, f64) = (126.0, 200.5);
const FEET_PER_UNIT: f64 = 2.44;

pub fn gameday_to_feet(cx: f64, cy: f64) -> (f64, f64) {
    (
        (cx - PLATE.0) * FEET_PER_UNIT,
        (PLATE.1 - cy) * FEET_PER_UNIT,
    )
}

/// Balls in play up to the cursor, in game order.
pub fn batted_balls(plays: &[Play], cursor: Cursor) -> Vec<BattedBall> {
    let mut out = Vec::new();
    for (i, play) in plays.iter().enumerate().take(cursor.play + 1) {
        let shown = if i < cursor.play {
            play.play_events.len()
        } else {
            cursor.events.min(play.play_events.len())
        };
        let whole = shown == play.play_events.len() && play.about.is_complete;
        for e in &play.play_events[..shown] {
            let Some(h) = &e.hit_data else { continue };
            let (Some(cx), Some(cy)) = (h.coordinates.coord_x, h.coordinates.coord_y) else {
                continue;
            };
            let (x, y) = gameday_to_feet(cx, cy);
            out.push(BattedBall {
                play: i,
                inning: play.about.inning,
                top: play.about.is_top_inning,
                batter: play.matchup.batter.id,
                batter_name: play.matchup.batter.full_name.clone(),
                event_type: if whole {
                    play.result.event_type.clone().unwrap_or_default()
                } else {
                    String::new()
                },
                result: if whole { notation(play) } else { "…".into() },
                launch_speed: h.launch_speed,
                launch_angle: h.launch_angle,
                distance: h.total_distance,
                trajectory: h.trajectory.clone(),
                x,
                y,
            });
        }
    }
    out
}

/// Statcast's barrel zone: at least 98 mph, with a launch-angle window
/// (26-30 degrees at 98) that widens as exit velocity rises, to 8-50 at 116+.
pub fn is_barrel(ev: f64, la: f64) -> bool {
    if ev < 98.0 {
        return false;
    }
    let (lo, hi) = match ev {
        e if e < 99.0 => (26.0, 30.0),
        e if e < 100.0 => (25.0, 31.0),
        e => {
            let over = (e - 100.0).min(16.0);
            (24.0 - over, 33.0 + over * 17.0 / 16.0)
        }
    };
    la >= lo && la <= hi
}

/// Last name for compact displays.
pub fn last_name(feed: &GameFeed, id: u64, fallback: &str) -> String {
    feed.game_data
        .players
        .get(&format!("ID{id}"))
        .map(|p| p.last_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| fallback.rsplit(' ').next().unwrap_or(fallback).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Play;

    fn play(json: &str) -> Play {
        serde_json::from_str(json).unwrap()
    }

    fn pitch(code: &str, balls: u32, strikes: u32) -> String {
        format!(
            r#"{{"isPitch":true,"type":"pitch","details":{{"call":{{"code":"{code}"}},"isBall":{}}},"count":{{"balls":{balls},"strikes":{strikes}}}}}"#,
            code == "B"
        )
    }

    fn single(batter: u64, inning: u32, top: bool) -> Play {
        play(&format!(
            r#"{{"result":{{"type":"atBat","eventType":"single","event":"Single"}},
               "about":{{"inning":{inning},"isTopInning":{top},"isComplete":true}},
               "matchup":{{"batter":{{"id":{batter}}},"pitcher":{{"id":9}}}},
               "playEvents":[{}],
               "runners":[{{"movement":{{"start":null,"end":"1B"}},"details":{{"runner":{{"id":{batter}}},"playIndex":0}}}}]}}"#,
            pitch("D", 0, 0)
        ))
    }

    #[test]
    fn segments_follow_runner_ids_not_bases() {
        // Runner on 2nd goes 2B->3B->score while the batter's runner from 1st
        // takes 2nd; listing order mixes them up.
        let p = play(
            r#"{"result":{"type":"atBat","eventType":"double"},
                "about":{"inning":1,"isTopInning":true,"isComplete":true},
                "matchup":{"batter":{"id":3},"pitcher":{"id":9}},
                "playEvents":[{"isPitch":true,"type":"pitch","details":{"call":{"code":"D"}}}],
                "runners":[
                 {"movement":{"start":"1B","end":"2B"},"details":{"runner":{"id":1},"playIndex":0}},
                 {"movement":{"start":null,"end":"2B"},"details":{"runner":{"id":3},"playIndex":0}},
                 {"movement":{"start":"2B","end":"3B"},"details":{"runner":{"id":2},"playIndex":0}},
                 {"movement":{"start":"3B","end":"score"},"details":{"runner":{"id":2},"playIndex":0}},
                 {"movement":{"start":"2B","end":"3B"},"details":{"runner":{"id":1},"playIndex":0}}
                ]}"#,
        );
        let s = Situation::at(&[p], Cursor { play: 0, events: 1 });
        assert_eq!(s.runs, [1, 0]);
        assert_eq!(s.bases, [None, Some(3), Some(1)]);
        assert_eq!(s.hits, [1, 0]);
    }

    #[test]
    fn half_inning_clears_bases_and_outs() {
        let plays = vec![single(1, 1, true), single(2, 1, false)];
        let s = Situation::at(&plays, Cursor::end(&plays).unwrap());
        assert_eq!(s.bases, [Some(2), None, None]);
        assert_eq!(s.line.len(), 1);
        assert_eq!(s.line[0], [Some(0), Some(0)]);
    }

    #[test]
    fn cursor_before_result_hides_it() {
        let plays = vec![single(1, 1, true)];
        let s = Situation::at(&plays, Cursor::start());
        assert_eq!(s.bases, [None; 3]);
        assert_eq!(s.hits, [0, 0]);
        assert!(!s.play_done);
    }

    #[test]
    fn cursor_walks_every_event() {
        let plays = vec![single(1, 1, true), single(2, 1, true)];
        let mut c = Cursor::start();
        let mut steps = 0;
        while let Some(n) = c.next(&plays) {
            c = n;
            steps += 1;
        }
        assert_eq!(steps, total_steps(&plays));
        assert_eq!(c, Cursor::end(&plays).unwrap());
        assert_eq!(c.ordinal(&plays), total_steps(&plays));
    }

    #[test]
    fn double_play_notation() {
        let p = play(
            r#"{"result":{"type":"atBat","eventType":"grounded_into_double_play"},
                "about":{"isComplete":true},
                "runners":[
                 {"movement":{"isOut":true,"outNumber":1},"credits":[{"credit":"f_assist","position":{"code":"6"}},{"credit":"f_putout","position":{"code":"4"}}]},
                 {"movement":{"isOut":true,"outNumber":2},"credits":[{"credit":"f_assist","position":{"code":"4"}},{"credit":"f_putout","position":{"code":"3"}}]}
                ]}"#,
        );
        assert_eq!(notation(&p), "6-4-3 DP");
    }

    #[test]
    fn fly_out_and_looking_strikeout() {
        let fly = play(
            r#"{"result":{"type":"atBat","eventType":"field_out"},
                "playEvents":[{"isPitch":true,"hitData":{"trajectory":"fly_ball"}}],
                "runners":[{"movement":{"isOut":true,"outNumber":1},"credits":[{"credit":"f_putout","position":{"code":"8"}}]}]}"#,
        );
        assert_eq!(notation(&fly), "F8");
        let k = play(&format!(
            r#"{{"result":{{"type":"atBat","eventType":"strikeout"}},"playEvents":[{}]}}"#,
            pitch("C", 1, 2)
        ));
        assert_eq!(notation(&k), "KL");
    }

    #[test]
    fn barrel_zone() {
        assert!(is_barrel(98.5, 28.0));
        assert!(!is_barrel(98.5, 20.0));
        assert!(!is_barrel(97.9, 28.0));
        assert!(is_barrel(110.0, 15.0));
        assert!(is_barrel(120.0, 9.0));
        assert!(!is_barrel(120.0, 55.0));
    }

    #[test]
    fn gameday_coordinates_to_feet() {
        let (x, y) = gameday_to_feet(126.0, 200.5);
        assert!(x.abs() < 1e-9 && y.abs() < 1e-9);
        // Straightaway center, 100 units out = 244 ft.
        let (x, y) = gameday_to_feet(126.0, 100.5);
        assert!(x.abs() < 1e-9 && (y - 244.0).abs() < 1e-9);
    }

    #[test]
    fn at_bats_skip_walks_and_sacrifices() {
        let r: Vec<String> = ["1B", "BB", "K", "SF", "HR"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(hits_for(&r), (2, 3));
    }
}

/// Checks the rebuilt state against the official linescore of real games.
/// Run with `BALLATUI_FIXTURES=<dir of feed/live JSON files> cargo test -- --ignored`.
#[cfg(test)]
mod fixture_tests {
    use super::*;

    #[test]
    #[ignore]
    fn rebuilt_linescore_matches_official() {
        let dir = std::env::var("BALLATUI_FIXTURES").expect("BALLATUI_FIXTURES");
        let mut failures = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let feed: GameFeed = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let plays = &feed.live_data.plays.all_plays;
            let s = Situation::at(plays, Cursor::end(plays).unwrap());
            let r = std::panic::catch_unwind(|| check(&feed, &s, &path));
            if r.is_err() {
                failures.push(path.display().to_string());
            }
        }
        assert!(
            failures.is_empty(),
            "{} failed: {failures:?}",
            failures.len()
        );
    }

    fn check(feed: &GameFeed, s: &Situation, path: &std::path::Path) {
        {
            let ls = &feed.live_data.linescore;
            let off = [&ls.teams.away, &ls.teams.home];
            #[allow(clippy::needless_range_loop)]
            for t in 0..2 {
                assert_eq!(Some(s.runs[t]), off[t].runs, "{} runs", path.display());
                assert_eq!(Some(s.hits[t]), off[t].hits, "{} hits", path.display());
                // Some errors (scoring changes made afterwards) never appear
                // in the play data, so the rebuilt count can only fall short.
                assert!(
                    s.errors[t] <= off[t].errors.unwrap_or(0),
                    "{} errors",
                    path.display()
                );
            }
            for (i, inn) in ls.innings.iter().enumerate() {
                assert_eq!(
                    s.line[i][0],
                    inn.away.runs,
                    "{} inning {}",
                    path.display(),
                    i + 1
                );
                if inn.home.runs.is_some() {
                    assert_eq!(
                        s.line[i][1],
                        inn.home.runs,
                        "{} inning {} home",
                        path.display(),
                        i + 1
                    );
                }
            }
            for home in [false, true] {
                let card = scorecard(feed, home, None);
                let runs: u32 = card.totals.values().map(|t| t.0).sum();
                assert_eq!(
                    Some(runs),
                    off[home as usize].runs,
                    "{} card runs",
                    path.display()
                );
                let scored = card
                    .rows
                    .iter()
                    .flat_map(|r| r.cells.values().flatten())
                    .filter(|c| c.reached == 4)
                    .count() as u32;
                assert_eq!(
                    Some(scored),
                    off[home as usize].runs,
                    "{} scored cells",
                    path.display()
                );
            }
            println!(
                "{}: ok {:?} {:?} {:?}",
                path.display(),
                s.runs,
                s.hits,
                s.errors
            );
        }
    }
}
