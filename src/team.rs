// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Everything on a team page: schedule around today, roster with season
//! stats, team leaders, and team stats ranked against the league.

use crate::model::{ScheduleGame, TeamInfo};
use crate::stats::{Stat, StatsResponse, num, text};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Teams {
    pub teams: Vec<TeamInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RosterResponse {
    pub roster: Vec<RosterEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RosterEntry {
    pub person: RosterPerson,
    pub jersey_number: String,
    pub position: crate::model::Position,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RosterPerson {
    pub id: u64,
    pub full_name: String,
    pub bat_side: Option<crate::model::CodeDesc>,
    pub pitch_hand: Option<crate::model::CodeDesc>,
    pub stats: Vec<crate::stats::StatBlock>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LeadersResponse {
    pub team_leaders: Vec<LeaderCategory>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LeaderCategory {
    pub leader_category: String,
    pub stat_group: String,
    pub leaders: Vec<Leader>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Leader {
    pub person: crate::model::PersonRef,
    pub value: String,
}

/// One roster player with his season lines.
#[derive(Debug, Clone)]
pub struct RosterPlayer {
    pub name: String,
    pub number: String,
    pub position: String,
    /// "Pitcher", "Catcher", "Infielder", "Outfielder", "Hitter", "Two-Way Player"
    pub position_type: String,
    pub bats: String,
    pub throws: String,
    pub hitting: Stat,
    pub pitching: Stat,
}

impl RosterPlayer {
    pub fn is_pitcher(&self) -> bool {
        self.position_type == "Pitcher" || self.position_type == "Two-Way Player"
    }
    pub fn is_hitter(&self) -> bool {
        self.position_type != "Pitcher"
    }
}

pub fn roster_from(r: &RosterResponse) -> Vec<RosterPlayer> {
    r.roster
        .iter()
        .map(|e| {
            let mut hitting = Stat::new();
            let mut pitching = Stat::new();
            for b in &e.person.stats {
                let s = b.splits.first().map(|s| s.stat.clone()).unwrap_or_default();
                match b.group.display_name.as_str() {
                    "hitting" => hitting = s,
                    "pitching" => pitching = s,
                    _ => {}
                }
            }
            RosterPlayer {
                name: e.person.full_name.clone(),
                number: e.jersey_number.clone(),
                position: e.position.abbreviation.clone(),
                position_type: e.position.kind.clone(),
                bats: e
                    .person
                    .bat_side
                    .as_ref()
                    .map(|x| x.code.clone())
                    .unwrap_or_default(),
                throws: e
                    .person
                    .pitch_hand
                    .as_ref()
                    .map(|x| x.code.clone())
                    .unwrap_or_default(),
                hitting,
                pitching,
            }
        })
        .collect()
}

/// The leader categories shown, with the stat group they come from.
pub const LEADER_CATEGORIES: [(&str, &str, &str); 9] = [
    ("homeRuns", "hitting", "HR"),
    ("battingAverage", "hitting", "AVG"),
    ("onBasePlusSlugging", "hitting", "OPS"),
    ("runsBattedIn", "hitting", "RBI"),
    ("stolenBases", "hitting", "SB"),
    ("earnedRunAverage", "pitching", "ERA"),
    ("strikeouts", "pitching", "K"),
    ("saves", "pitching", "SV"),
    ("wins", "pitching", "W"),
];

/// Leaders per shown category: (label, [(name, value)]).
pub fn leaders_from(r: &LeadersResponse) -> Vec<(String, Vec<(String, String)>)> {
    LEADER_CATEGORIES
        .iter()
        .map(|(cat, group, label)| {
            let list = r
                .team_leaders
                .iter()
                .find(|c| c.leader_category == *cat && c.stat_group == *group)
                .map(|c| {
                    c.leaders
                        .iter()
                        .map(|l| (l.person.full_name.clone(), l.value.clone()))
                        .collect()
                })
                .unwrap_or_default();
            (label.to_string(), list)
        })
        .collect()
}

/// A team stat with its rank among all teams (1 = best).
#[derive(Debug, Clone)]
pub struct Ranked {
    pub label: &'static str,
    pub value: String,
    pub rank: usize,
    pub of: usize,
    pub pitching: bool,
}

/// (label, stat key, group is pitching, higher is better, per-PA rate)
const RANKED: [(&str, &str, bool, bool, bool); 14] = [
    ("Runs", "runs", false, true, false),
    ("AVG", "avg", false, true, false),
    ("OBP", "obp", false, true, false),
    ("SLG", "slg", false, true, false),
    ("OPS", "ops", false, true, false),
    ("HR", "homeRuns", false, true, false),
    ("SB", "stolenBases", false, true, false),
    ("K%", "strikeOuts", false, false, true),
    ("ERA", "era", true, false, false),
    ("WHIP", "whip", true, false, false),
    ("Runs allowed", "runs", true, false, false),
    ("K%", "strikeOuts", true, true, true),
    ("BB%", "baseOnBalls", true, false, true),
    ("HR allowed", "homeRuns", true, false, false),
];

/// Team id of a team-stats row.
fn row_team(s: &crate::stats::StatSplit) -> Option<u64> {
    s.team.as_ref().map(|t| t.id)
}

pub fn ranks_from(team: u64, hitting: &StatsResponse, pitching: &StatsResponse) -> Vec<Ranked> {
    let rows = |r: &StatsResponse| -> Vec<(u64, Stat)> {
        r.stats
            .iter()
            .flat_map(|b| b.splits.iter())
            .filter_map(|s| Some((row_team(s)?, s.stat.clone())))
            .collect()
    };
    let h = rows(hitting);
    let p = rows(pitching);
    let mut out = Vec::new();
    for (label, key, pitch, higher, rate) in RANKED {
        let rows = if pitch { &p } else { &h };
        let value_of = |s: &Stat| -> Option<f64> {
            if rate {
                let pa = num(s, "plateAppearances").or_else(|| num(s, "battersFaced"))?;
                Some(num(s, key)? / pa.max(1.0))
            } else {
                num(s, key)
            }
        };
        let Some(mine) = rows.iter().find(|(id, _)| *id == team) else {
            continue;
        };
        let Some(v) = value_of(&mine.1) else { continue };
        let all: Vec<f64> = rows.iter().filter_map(|(_, s)| value_of(s)).collect();
        let better = all
            .iter()
            .filter(|x| if higher { **x > v } else { **x < v })
            .count();
        let shown = if rate {
            format!("{:.1}%", v * 100.0)
        } else {
            text(&mine.1, key).unwrap_or_default()
        };
        out.push(Ranked {
            label,
            value: shown,
            rank: better + 1,
            of: all.len(),
            pitching: pitch,
        });
    }
    out
}

/// Everything shown on one team page.
#[derive(Debug, Clone, Default)]
pub struct TeamPage {
    pub games: Vec<ScheduleGame>,
    pub roster: Vec<RosterPlayer>,
    pub leaders: Vec<(String, Vec<(String, String)>)>,
    pub hitting_ranks: Vec<Ranked>,
    pub pitching_ranks: Vec<Ranked>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_count_better_teams() {
        let h: StatsResponse = serde_json::from_str(
            r#"{"stats":[{"splits":[
              {"team":{"id":1},"stat":{"runs":700,"avg":".250","plateAppearances":6000,"strikeOuts":1200}},
              {"team":{"id":2},"stat":{"runs":800,"avg":".260","plateAppearances":6000,"strikeOuts":1500}},
              {"team":{"id":3},"stat":{"runs":600,"avg":".240","plateAppearances":6000,"strikeOuts":1000}}]}]}"#,
        )
        .unwrap();
        let p: StatsResponse = serde_json::from_str(r#"{"stats":[]}"#).unwrap();
        let r = ranks_from(1, &h, &p);
        let runs = r.iter().find(|x| x.label == "Runs").unwrap();
        assert_eq!((runs.rank, runs.of), (2, 3));
        // Fewer strikeouts rank higher for hitters.
        let k = r.iter().find(|x| x.label == "K%").unwrap();
        assert_eq!(k.rank, 2);
        assert_eq!(k.value, "20.0%");
    }

    #[test]
    fn leaders_pick_the_right_group() {
        let r: LeadersResponse = serde_json::from_str(
            r#"{"teamLeaders":[
              {"leaderCategory":"strikeouts","statGroup":"hitting","leaders":[{"person":{"fullName":"A"},"value":"165"}]},
              {"leaderCategory":"strikeouts","statGroup":"pitching","leaders":[{"person":{"fullName":"B"},"value":"171"}]}]}"#,
        )
        .unwrap();
        let l = leaders_from(&r);
        let k = l.iter().find(|(lab, _)| lab == "K").unwrap();
        assert_eq!(k.1[0].0, "B");
    }
}
