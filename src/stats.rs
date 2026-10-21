// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Season and advanced stats for the matchup view: the API's own
//! sabermetrics (wOBA, wRC+, FIP, xFIP, WAR), Statcast expected stats,
//! platoon splits, pitch arsenal, head-to-head history and league averages.

use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

pub type Stat = Map<String, Value>;

/// Reads a number that the API may send as a number or a string (".272").
pub fn num(s: &Stat, key: &str) -> Option<f64> {
    match s.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(t) => t.trim().parse().ok(),
        _ => None,
    }
}

pub fn text(s: &Stat, key: &str) -> Option<String> {
    match s.get(key)? {
        Value::String(t) => Some(t.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

// ------------------------------------------------------------ API shapes

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct People {
    pub people: Vec<Person>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Person {
    pub id: u64,
    pub full_name: String,
    pub primary_position: Option<crate::model::Position>,
    pub bat_side: Option<crate::model::CodeDesc>,
    pub pitch_hand: Option<crate::model::CodeDesc>,
    pub stats: Vec<StatBlock>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatBlock {
    #[serde(rename = "type")]
    pub kind: DisplayName,
    pub group: DisplayName,
    pub splits: Vec<StatSplit>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DisplayName {
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatSplit {
    pub stat: Stat,
    pub split: Option<crate::model::CodeDesc>,
    /// Set on the combined line of a player who played for several teams.
    pub num_teams: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatsResponse {
    pub stats: Vec<StatBlock>,
}

// ------------------------------------------------------------ app shapes

/// One stat group (hitting or pitching) for a season.
#[derive(Debug, Clone, Default)]
pub struct GroupStats {
    pub season: Stat,
    pub saber: Stat,
    pub expected: Stat,
    /// Against left-handed / right-handed opponents.
    pub vs_left: Stat,
    pub vs_right: Stat,
}

impl GroupStats {
    pub fn is_empty(&self) -> bool {
        self.season.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayerStats {
    pub id: u64,
    pub name: String,
    pub position: String,
    pub bats: String,
    pub throws: String,
    pub hitting: GroupStats,
    pub pitching: GroupStats,
}

/// Picks the line to show: the combined line for a traded player, otherwise
/// the only one.
fn pick(splits: &[StatSplit], code: Option<&str>) -> Stat {
    let matching: Vec<&StatSplit> = splits
        .iter()
        .filter(|s| match code {
            Some(c) => s.split.as_ref().is_some_and(|x| x.code == c),
            None => true,
        })
        .collect();
    matching
        .iter()
        .find(|s| s.num_teams.is_some())
        .or(matching.first())
        .map(|s| s.stat.clone())
        .unwrap_or_default()
}

impl PlayerStats {
    pub fn from_person(p: &Person) -> Self {
        let mut out = PlayerStats {
            id: p.id,
            name: p.full_name.clone(),
            position: p
                .primary_position
                .as_ref()
                .map(|x| x.abbreviation.clone())
                .unwrap_or_default(),
            bats: p
                .bat_side
                .as_ref()
                .map(|x| x.code.clone())
                .unwrap_or_default(),
            throws: p
                .pitch_hand
                .as_ref()
                .map(|x| x.code.clone())
                .unwrap_or_default(),
            ..Default::default()
        };
        for b in &p.stats {
            let g = match b.group.display_name.as_str() {
                "hitting" => &mut out.hitting,
                "pitching" => &mut out.pitching,
                _ => continue,
            };
            match b.kind.display_name.as_str() {
                "season" => g.season = pick(&b.splits, None),
                "sabermetrics" => g.saber = pick(&b.splits, None),
                "expectedStatistics" => g.expected = pick(&b.splits, None),
                "statSplits" => {
                    g.vs_left = pick(&b.splits, Some("vl"));
                    g.vs_right = pick(&b.splits, Some("vr"));
                }
                _ => {}
            }
        }
        out
    }
}

/// One pitch type in a pitcher's season mix.
#[derive(Debug, Clone)]
pub struct ArsenalPitch {
    pub code: String,
    pub name: String,
    pub share: f64,
    pub mph: f64,
}

pub fn arsenal_from(resp: &StatsResponse) -> Vec<ArsenalPitch> {
    let mut out: Vec<ArsenalPitch> = resp
        .stats
        .iter()
        .flat_map(|b| b.splits.iter())
        .filter_map(|s| {
            let t = s.stat.get("type")?;
            Some(ArsenalPitch {
                code: t.get("code")?.as_str()?.to_string(),
                name: t.get("description")?.as_str()?.to_string(),
                share: num(&s.stat, "percentage")?,
                mph: num(&s.stat, "averageSpeed").unwrap_or(0.0),
            })
        })
        .collect();
    out.sort_by(|a, b| b.share.total_cmp(&a.share));
    out
}

/// Career totals of a batter against a pitcher, if they've met.
pub fn head_to_head_from(resp: &StatsResponse) -> Option<Stat> {
    resp.stats
        .iter()
        .find(|b| b.kind.display_name == "vsPlayerTotal")
        .and_then(|b| b.splits.first())
        .map(|s| s.stat.clone())
}

/// League-wide rates for the season, to mark "average" on the bars.
#[derive(Debug, Clone, Copy, Default)]
pub struct League {
    pub k_pct: f64,
    pub bb_pct: f64,
    pub avg: f64,
    pub obp: f64,
    pub slg: f64,
    pub babip: f64,
    pub era: f64,
}

impl League {
    /// FanGraphs scales wOBA so the league average equals league OBP.
    pub fn woba(&self) -> f64 {
        self.obp
    }
    pub fn iso(&self) -> f64 {
        self.slg - self.avg
    }
}

pub fn league_from(hitting: &StatsResponse, pitching: &StatsResponse) -> League {
    let sum = |r: &StatsResponse, k: &str| -> f64 {
        r.stats
            .iter()
            .flat_map(|b| b.splits.iter())
            .filter_map(|s| num(&s.stat, k))
            .sum()
    };
    let pa = sum(hitting, "plateAppearances").max(1.0);
    let ab = sum(hitting, "atBats").max(1.0);
    let h = sum(hitting, "hits");
    let bb = sum(hitting, "baseOnBalls");
    let hbp = sum(hitting, "hitByPitch");
    let sf = sum(hitting, "sacFlies");
    let k = sum(hitting, "strikeOuts");
    let hr = sum(hitting, "homeRuns");
    let tb = sum(hitting, "totalBases");
    let er = sum(pitching, "earnedRuns");
    let outs = sum(pitching, "outs").max(1.0);
    League {
        k_pct: k / pa,
        bb_pct: bb / pa,
        avg: h / ab,
        obp: (h + bb + hbp) / (ab + bb + hbp + sf).max(1.0),
        slg: tb / ab,
        babip: (h - hr) / (ab - k - hr + sf).max(1.0),
        era: er * 27.0 / outs,
    }
}

// ----------------------------------------------------------------- cache

#[derive(Debug, Clone)]
pub enum Load<T> {
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Load<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(t) => Some(t),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct StatsCache {
    pub players: HashMap<(u64, i32), Load<PlayerStats>>,
    pub arsenal: HashMap<(u64, i32), Load<Vec<ArsenalPitch>>>,
    pub head_to_head: HashMap<(u64, u64), Load<Option<Stat>>>,
    pub league: HashMap<i32, Load<League>>,
}

impl StatsCache {
    pub fn retry_failed(&mut self) {
        self.players.retain(|_, v| !matches!(v, Load::Failed(_)));
        self.arsenal.retain(|_, v| !matches!(v, Load::Failed(_)));
        self.head_to_head
            .retain(|_, v| !matches!(v, Load::Failed(_)));
        self.league.retain(|_, v| !matches!(v, Load::Failed(_)));
    }

    pub fn player(&self, id: u64, season: i32) -> Option<&PlayerStats> {
        self.players.get(&(id, season))?.ready()
    }
    pub fn league(&self, season: i32) -> Option<League> {
        self.league.get(&season)?.ready().copied()
    }
}

// ---------------------------------------------------------- derived rates

/// Rate stats derived from a season line.
pub fn per_pa(s: &Stat, key: &str) -> Option<f64> {
    let pa = num(s, "plateAppearances").or_else(|| num(s, "battersFaced"))?;
    if pa <= 0.0 {
        return None;
    }
    Some(num(s, key)? / pa)
}

pub fn iso(s: &Stat) -> Option<f64> {
    Some(num(s, "slg")? - num(s, "avg")?)
}

/// `.272/.350/.512` from a hitting (or opponents') line.
pub fn slash(s: &Stat) -> Option<String> {
    Some(format!(
        "{}/{}/{}",
        text(s, "avg")?,
        text(s, "obp")?,
        text(s, "slg")?
    ))
}

/// `.375`-style formatting for rates below one.
pub fn rate3(v: f64) -> String {
    let s = format!("{v:.3}");
    s.strip_prefix('0').map(|t| t.to_string()).unwrap_or(s)
}

pub fn pct1(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traded_player_uses_combined_line() {
        let p: Person = serde_json::from_str(
            r#"{"id":1,"fullName":"A B","stats":[
              {"type":{"displayName":"season"},"group":{"displayName":"pitching"},"splits":[
                {"stat":{"era":"3.00"}},
                {"stat":{"era":"4.42"},"numTeams":2},
                {"stat":{"era":"5.00"}}]},
              {"type":{"displayName":"statSplits"},"group":{"displayName":"pitching"},"splits":[
                {"stat":{"ops":".700"},"split":{"code":"vl"}},
                {"stat":{"ops":".757"},"split":{"code":"vl"},"numTeams":2},
                {"stat":{"ops":".688"},"split":{"code":"vr"},"numTeams":2}]}]}"#,
        )
        .unwrap();
        let s = PlayerStats::from_person(&p);
        assert_eq!(num(&s.pitching.season, "era"), Some(4.42));
        assert_eq!(text(&s.pitching.vs_left, "ops").as_deref(), Some(".757"));
        assert_eq!(text(&s.pitching.vs_right, "ops").as_deref(), Some(".688"));
        assert!(s.hitting.is_empty());
    }

    #[test]
    fn numbers_from_strings_and_formatting() {
        let s: Stat =
            serde_json::from_str(r#"{"avg":".272","pa":"-.--","k":10,"plateAppearances":50}"#)
                .unwrap();
        assert_eq!(num(&s, "avg"), Some(0.272));
        assert_eq!(num(&s, "pa"), None);
        assert_eq!(per_pa(&s, "k"), Some(0.2));
        assert_eq!(rate3(0.3754), ".375");
        assert_eq!(rate3(1.2), "1.200");
        assert_eq!(pct1(0.2213), "22.1%");
    }

    #[test]
    fn arsenal_sorted_by_usage() {
        let r: StatsResponse = serde_json::from_str(
            r#"{"stats":[{"splits":[
              {"stat":{"percentage":0.2,"averageSpeed":88.0,"type":{"code":"SL","description":"Slider"}}},
              {"stat":{"percentage":0.5,"averageSpeed":95.8,"type":{"code":"FF","description":"Four-seam FB"}}}]}]}"#,
        )
        .unwrap();
        let a = arsenal_from(&r);
        assert_eq!(a[0].code, "FF");
        assert_eq!(a.len(), 2);
    }
}
