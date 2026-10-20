// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Blocking client for the public MLB Stats API.

use crate::model::{GameFeed, Schedule, Standings};
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

    pub fn standings(&self, season: i32, kind: &str) -> Result<Standings> {
        self.get(&format!(
            "/api/v1/standings?leagueId=103,104&season={season}\
             &standingsTypes={kind}&hydrate=team"
        ))
    }
}
