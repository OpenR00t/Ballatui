// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Blocking client for the public MLB Stats API.

use crate::model::{FieldInfo, GameFeed, Schedule, Standings, Venues};
use crate::stats::{
    ArsenalPitch, League, People, PlayerStats, Stat, StatsResponse, arsenal_from,
    head_to_head_from, league_from,
};
use anyhow::{Context, Result};
use std::time::Duration;

const DEFAULT_BASE: &str = "https://statsapi.mlb.com";

#[derive(Clone)]
pub struct Api {
    http: reqwest::blocking::Client,
    base: String,
}

impl Api {
    pub fn new() -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("ballatui/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()?;
        // BALLATUI_API points the app at another server (a local mirror or a
        // test fixture server).
        let base = std::env::var("BALLATUI_API")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE.to_string());
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
        })
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .with_context(|| format!("requesting {path}"))?
            .error_for_status()
            .with_context(|| format!("requesting {path}"))?;
        let bytes = resp.bytes()?;
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {path}"))
    }

    pub fn schedule(&self, date: &str) -> Result<Schedule> {
        self.get(&format!(
            "/api/v1/schedule?sportId=1&date={date}\
             &hydrate=team,linescore,probablePitcher,decisions,seriesStatus"
        ))
    }

    /// Dates between `start` and `end` (inclusive) that have games.
    pub fn game_dates(&self, start: &str, end: &str) -> Result<Vec<String>> {
        let s: Schedule = self.get(&format!(
            "/api/v1/schedule?sportId=1&startDate={start}&endDate={end}"
        ))?;
        Ok(s.dates
            .into_iter()
            .filter(|d| !d.games.is_empty())
            .map(|d| d.date)
            .collect())
    }

    pub fn feed(&self, game_pk: u64) -> Result<GameFeed> {
        self.get(&format!("/api/v1.1/game/{game_pk}/feed/live"))
    }

    /// Season, advanced, expected and platoon stats for several players in
    /// one request.
    pub fn player_stats(&self, ids: &[u64], season: i32) -> Result<Vec<PlayerStats>> {
        let ids: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
        let people: People = self.get(&format!(
            "/api/v1/people?personIds={}&hydrate=stats(group=[hitting,pitching],\
             type=[season,sabermetrics,expectedStatistics,statSplits],sitCodes=[vl,vr],\
             season={season},gameType=R)",
            ids.join(",")
        ))?;
        Ok(people.people.iter().map(PlayerStats::from_person).collect())
    }

    pub fn arsenal(&self, pitcher: u64, season: i32) -> Result<Vec<ArsenalPitch>> {
        let r: StatsResponse = self.get(&format!(
            "/api/v1/people/{pitcher}/stats?stats=pitchArsenal&group=pitching&season={season}"
        ))?;
        Ok(arsenal_from(&r))
    }

    pub fn head_to_head(&self, batter: u64, pitcher: u64) -> Result<Option<Stat>> {
        let r: StatsResponse = self.get(&format!(
            "/api/v1/people/{batter}/stats?stats=vsPlayer&opposingPlayerId={pitcher}&group=hitting"
        ))?;
        Ok(head_to_head_from(&r))
    }

    pub fn league(&self, season: i32) -> Result<League> {
        let path = |g: &str| {
            format!(
                "/api/v1/teams/stats?stats=season&group={g}&season={season}&sportIds=1&gameType=R"
            )
        };
        let h: StatsResponse = self.get(&path("hitting"))?;
        let p: StatsResponse = self.get(&path("pitching"))?;
        Ok(league_from(&h, &p))
    }

    pub fn venue(&self, id: u64) -> Result<Option<FieldInfo>> {
        let v: Venues = self.get(&format!("/api/v1/venues/{id}?hydrate=fieldInfo"))?;
        Ok(v.venues.into_iter().next().and_then(|v| v.field_info))
    }

    pub fn standings(&self, season: i32, kind: &str) -> Result<Standings> {
        self.get(&format!(
            "/api/v1/standings?leagueId=103,104&season={season}\
             &standingsTypes={kind}&hydrate=team"
        ))
    }
}
