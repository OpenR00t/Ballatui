// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! The parts of the MLB Stats API responses the app uses. Every field has a
//! default so missing or new fields never break parsing.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonRef {
    pub id: u64,
    pub full_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IdRef {
    pub id: u64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeDesc {
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    pub abstract_game_state: String,
    pub detailed_state: String,
    pub coded_game_state: String,
    pub status_code: String,
    pub reason: Option<String>,
}

impl Status {
    pub fn is_live(&self) -> bool {
        self.abstract_game_state == "Live"
    }
    pub fn is_final(&self) -> bool {
        self.abstract_game_state == "Final"
    }
    pub fn is_preview(&self) -> bool {
        self.abstract_game_state == "Preview"
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WinLoss {
    pub wins: u32,
    pub losses: u32,
    pub pct: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamInfo {
    pub id: u64,
    pub name: String,
    pub abbreviation: String,
    pub team_name: String,
    pub location_name: String,
    pub short_name: String,
    pub record: Option<TeamSeasonRecord>,
    pub league: Option<IdRef>,
    pub division: Option<IdRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamSeasonRecord {
    pub wins: u32,
    pub losses: u32,
}

// ---------------------------------------------------------------- schedule

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    pub dates: Vec<ScheduleDate>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleDate {
    pub date: String,
    pub games: Vec<ScheduleGame>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleGame {
    pub game_pk: u64,
    pub game_type: String,
    pub game_date: String,
    pub official_date: String,
    pub status: Status,
    pub teams: Sides<ScheduleSide>,
    pub linescore: Option<Linescore>,
    pub venue: IdRef,
    pub description: Option<String>,
    pub series_description: Option<String>,
    pub series_game_number: Option<u32>,
    pub games_in_series: Option<u32>,
    pub double_header: String,
    pub game_number: u32,
    pub series_status: Option<SeriesStatus>,
    pub decisions: Option<Decisions>,
    pub scheduled_innings: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SeriesStatus {
    pub result: Option<String>,
    pub description: Option<String>,
    pub short_description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sides<T> {
    pub away: T,
    pub home: T,
}

impl<T> Sides<T> {
    pub fn get(&self, home: bool) -> &T {
        if home { &self.home } else { &self.away }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleSide {
    pub team: TeamInfo,
    pub score: Option<u32>,
    pub is_winner: Option<bool>,
    pub league_record: Option<WinLoss>,
    pub probable_pitcher: Option<PersonRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Decisions {
    pub winner: Option<PersonRef>,
    pub loser: Option<PersonRef>,
    pub save: Option<PersonRef>,
}

// --------------------------------------------------------------- linescore

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Linescore {
    pub current_inning: Option<u32>,
    pub current_inning_ordinal: Option<String>,
    pub inning_state: Option<String>,
    pub inning_half: Option<String>,
    pub is_top_inning: Option<bool>,
    pub scheduled_innings: Option<u32>,
    pub innings: Vec<LineInning>,
    pub teams: Sides<LineTeam>,
    pub balls: Option<u32>,
    pub strikes: Option<u32>,
    pub outs: Option<u32>,
    pub offense: Option<Offense>,
    pub defense: Option<Defense>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LineInning {
    pub num: u32,
    pub home: LineTeam,
    pub away: LineTeam,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LineTeam {
    pub runs: Option<u32>,
    pub hits: Option<u32>,
    pub errors: Option<u32>,
    pub left_on_base: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Offense {
    pub batter: Option<PersonRef>,
    pub on_deck: Option<PersonRef>,
    pub in_hole: Option<PersonRef>,
    pub first: Option<PersonRef>,
    pub second: Option<PersonRef>,
    pub third: Option<PersonRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Defense {
    pub pitcher: Option<PersonRef>,
}

// -------------------------------------------------------------- live feed

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameFeed {
    pub game_pk: u64,
    pub meta_data: MetaData,
    pub game_data: GameData,
    pub live_data: LiveData,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetaData {
    pub wait: Option<u64>,
    pub time_stamp: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameData {
    pub game: GameInfo,
    pub datetime: GameDateTime,
    pub status: Status,
    pub teams: Sides<TeamInfo>,
    pub players: HashMap<String, PlayerInfo>,
    pub venue: IdRef,
    pub weather: Option<Weather>,
    pub probable_pitchers: Option<Sides<Option<PersonRef>>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameInfo {
    pub pk: u64,
    #[serde(rename = "type")]
    pub game_type: String,
    pub season: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameDateTime {
    pub date_time: String,
    pub official_date: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Weather {
    pub condition: Option<String>,
    pub temp: Option<String>,
    pub wind: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayerInfo {
    pub id: u64,
    pub full_name: String,
    pub last_name: String,
    pub boxscore_name: String,
    pub primary_number: Option<String>,
    pub bat_side: Option<CodeDesc>,
    pub pitch_hand: Option<CodeDesc>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveData {
    pub plays: Plays,
    pub linescore: Linescore,
    pub boxscore: Boxscore,
    pub decisions: Option<Decisions>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Plays {
    pub all_plays: Vec<Play>,
    pub scoring_plays: Vec<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Play {
    pub result: PlayResult,
    pub about: About,
    pub count: Count,
    pub matchup: Matchup,
    pub runners: Vec<Runner>,
    pub play_events: Vec<PlayEvent>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayResult {
    #[serde(rename = "type")]
    pub kind: String,
    pub event: Option<String>,
    pub event_type: Option<String>,
    pub description: Option<String>,
    pub rbi: u32,
    pub away_score: Option<u32>,
    pub home_score: Option<u32>,
    pub is_out: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct About {
    pub at_bat_index: usize,
    pub half_inning: String,
    pub is_top_inning: bool,
    pub inning: u32,
    pub is_complete: bool,
    pub is_scoring_play: bool,
    pub has_out: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Count {
    pub balls: u32,
    pub strikes: u32,
    pub outs: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Matchup {
    pub batter: PersonRef,
    pub bat_side: CodeDesc,
    pub pitcher: PersonRef,
    pub pitch_hand: CodeDesc,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Runner {
    pub movement: Movement,
    pub details: RunnerDetails,
    pub credits: Vec<Credit>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Movement {
    pub origin_base: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub out_base: Option<String>,
    pub is_out: Option<bool>,
    pub out_number: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerDetails {
    pub event: Option<String>,
    pub event_type: Option<String>,
    pub movement_reason: Option<String>,
    pub runner: PersonRef,
    pub is_scoring_event: bool,
    pub rbi: bool,
    pub earned: bool,
    pub play_index: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Credit {
    pub player: IdRef,
    pub position: Position,
    pub credit: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Position {
    pub code: String,
    pub abbreviation: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayEvent {
    pub details: EventDetails,
    pub count: Count,
    pub pitch_data: Option<PitchData>,
    pub hit_data: Option<HitData>,
    pub index: usize,
    pub pitch_number: Option<u32>,
    pub is_pitch: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub player: Option<PersonRef>,
    pub replaced_player: Option<PersonRef>,
    pub is_substitution: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EventDetails {
    pub call: Option<CodeDesc>,
    pub description: Option<String>,
    pub code: Option<String>,
    pub event: Option<String>,
    pub event_type: Option<String>,
    pub is_in_play: bool,
    pub is_strike: bool,
    pub is_ball: bool,
    #[serde(rename = "type")]
    pub pitch_type: Option<CodeDesc>,
    pub is_scoring_play: Option<bool>,
    pub away_score: Option<u32>,
    pub home_score: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PitchData {
    pub start_speed: Option<f64>,
    pub end_speed: Option<f64>,
    pub strike_zone_top: Option<f64>,
    pub strike_zone_bottom: Option<f64>,
    pub coordinates: PitchCoords,
    pub breaks: Option<Breaks>,
    pub zone: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PitchCoords {
    #[serde(rename = "pX")]
    pub px: Option<f64>,
    #[serde(rename = "pZ")]
    pub pz: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Breaks {
    pub spin_rate: Option<f64>,
    pub break_vertical_induced: Option<f64>,
    /// Inches, catcher's view: positive toward third base (a right-hander's
    /// arm side).
    pub break_horizontal: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HitData {
    pub launch_speed: Option<f64>,
    pub launch_angle: Option<f64>,
    pub total_distance: Option<f64>,
    pub trajectory: Option<String>,
    pub location: Option<String>,
    pub coordinates: HitCoords,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HitCoords {
    pub coord_x: Option<f64>,
    pub coord_y: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Venues {
    pub venues: Vec<Venue>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Venue {
    pub id: u64,
    pub name: String,
    pub field_info: Option<FieldInfo>,
}

/// Fence distances in feet. `left`/`right` (between the line and the gap)
/// are missing for some parks.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldInfo {
    pub left_line: Option<f64>,
    pub left: Option<f64>,
    pub left_center: Option<f64>,
    pub center: Option<f64>,
    pub right_center: Option<f64>,
    pub right: Option<f64>,
    pub right_line: Option<f64>,
}

impl FieldInfo {
    /// Fence distance at an angle from straightaway center (degrees,
    /// negative toward left field), interpolated between the known points.
    pub fn fence_at(&self, angle: f64) -> f64 {
        let ll = self.left_line.unwrap_or(330.0);
        let lc = self.left_center.unwrap_or(375.0);
        let c = self.center.unwrap_or(400.0);
        let rc = self.right_center.unwrap_or(375.0);
        let rl = self.right_line.unwrap_or(330.0);
        let mut pts: Vec<(f64, f64)> = vec![(-45.0, ll)];
        if let Some(l) = self.left {
            pts.push((-34.0, l));
        }
        pts.extend([(-22.5, lc), (0.0, c), (22.5, rc)]);
        if let Some(r) = self.right {
            pts.push((34.0, r));
        }
        pts.push((45.0, rl));
        let a = angle.clamp(-45.0, 45.0);
        for w in pts.windows(2) {
            let ((a0, d0), (a1, d1)) = (w[0], w[1]);
            if a >= a0 && a <= a1 {
                return d0 + (d1 - d0) * (a - a0) / (a1 - a0);
            }
        }
        c
    }
}

// ---------------------------------------------------------------- boxscore

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Boxscore {
    pub teams: Sides<BoxTeam>,
    pub info: Vec<LabelValue>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelValue {
    pub label: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoxTeam {
    pub team: TeamInfo,
    pub team_stats: StatGroups,
    pub players: HashMap<String, BoxPlayer>,
    pub batters: Vec<u64>,
    pub pitchers: Vec<u64>,
    pub batting_order: Vec<u64>,
    pub info: Vec<InfoSection>,
    pub note: Vec<LabelValue>,
}

impl BoxTeam {
    pub fn player(&self, id: u64) -> Option<&BoxPlayer> {
        self.players.get(&format!("ID{id}"))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InfoSection {
    pub title: String,
    pub field_list: Vec<LabelValue>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatGroups {
    pub batting: BattingStats,
    pub pitching: PitchingStats,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoxPlayer {
    pub person: PersonRef,
    pub jersey_number: String,
    pub position: Position,
    pub batting_order: Option<String>,
    pub stats: StatGroups,
    pub season_stats: StatGroups,
    pub all_positions: Vec<Position>,
}

impl BoxPlayer {
    /// Lineup slot 1-9 and whether the player came in as a substitute.
    pub fn lineup_slot(&self) -> Option<(usize, bool)> {
        let n: usize = self.batting_order.as_deref()?.parse().ok()?;
        Some((n / 100, !n.is_multiple_of(100)))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BattingStats {
    pub summary: Option<String>,
    pub games_played: Option<u32>,
    pub plate_appearances: Option<u32>,
    pub at_bats: Option<u32>,
    pub runs: Option<u32>,
    pub hits: Option<u32>,
    pub doubles: Option<u32>,
    pub triples: Option<u32>,
    pub home_runs: Option<u32>,
    pub rbi: Option<u32>,
    pub base_on_balls: Option<u32>,
    pub intentional_walks: Option<u32>,
    pub strike_outs: Option<u32>,
    pub hit_by_pitch: Option<u32>,
    pub sac_flies: Option<u32>,
    pub stolen_bases: Option<u32>,
    pub left_on_base: Option<u32>,
    pub total_bases: Option<u32>,
    pub avg: Option<String>,
    pub obp: Option<String>,
    pub slg: Option<String>,
    pub ops: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PitchingStats {
    pub note: Option<String>,
    pub summary: Option<String>,
    pub games_played: Option<u32>,
    pub innings_pitched: Option<String>,
    pub hits: Option<u32>,
    pub runs: Option<u32>,
    pub earned_runs: Option<u32>,
    pub base_on_balls: Option<u32>,
    pub intentional_walks: Option<u32>,
    pub strike_outs: Option<u32>,
    pub home_runs: Option<u32>,
    pub hit_batsmen: Option<u32>,
    pub hit_by_pitch: Option<u32>,
    pub number_of_pitches: Option<u32>,
    pub pitches_thrown: Option<u32>,
    pub strikes: Option<u32>,
    pub outs: Option<u32>,
    pub batters_faced: Option<u32>,
    pub wins: Option<u32>,
    pub losses: Option<u32>,
    pub saves: Option<u32>,
    pub era: Option<String>,
    pub whip: Option<String>,
}

// --------------------------------------------------------------- standings

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Standings {
    pub records: Vec<StandingsRecord>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StandingsRecord {
    pub standings_type: String,
    pub league: IdRef,
    pub division: Option<IdRef>,
    pub team_records: Vec<TeamRecord>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamRecord {
    pub team: TeamInfo,
    pub streak: Option<Streak>,
    pub games_back: String,
    pub wild_card_games_back: Option<String>,
    pub wins: u32,
    pub losses: u32,
    pub winning_percentage: String,
    pub run_differential: i32,
    pub clinch_indicator: Option<String>,
    pub division_rank: Option<String>,
    pub wild_card_rank: Option<String>,
    pub elimination_number: Option<String>,
    pub records: SplitRecords,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Streak {
    pub streak_code: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SplitRecords {
    pub split_records: Vec<SplitRecord>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SplitRecord {
    #[serde(rename = "type")]
    pub kind: String,
    pub wins: u32,
    pub losses: u32,
}

impl TeamRecord {
    pub fn split(&self, kind: &str) -> Option<&SplitRecord> {
        self.records.split_records.iter().find(|s| s.kind == kind)
    }
}

pub fn division_name(id: u64) -> &'static str {
    match id {
        201 => "AL East",
        202 => "AL Central",
        200 => "AL West",
        204 => "NL East",
        205 => "NL Central",
        203 => "NL West",
        _ => "Division",
    }
}

pub fn league_name(id: u64) -> &'static str {
    match id {
        103 => "American League",
        104 => "National League",
        _ => "League",
    }
}

// --------------------------------------------------------- win probability

/// Win probability after one plate appearance.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WpPoint {
    pub at_bat_index: usize,
    /// Percent, after the play.
    pub home_team_win_probability: f64,
    /// Percentage points the play moved it (positive helps the home team).
    pub home_team_win_probability_added: f64,
    /// How much the situation mattered going in (1.0 = average).
    pub leverage_index: Option<f64>,
}
