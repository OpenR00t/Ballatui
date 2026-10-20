// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! `config.toml` in the platform config directory. Created with defaults on
//! first run so it is easy to find and edit.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Team abbreviations (e.g. "SEA", "CHC") pinned to the top of the
    /// scoreboard and highlighted in standings.
    pub favorites: Vec<String>,
    /// Seconds between refreshes of a live game.
    pub live_poll_seconds: u64,
    /// Seconds between scoreboard refreshes while any shown game is live.
    pub scoreboard_poll_seconds: u64,
    /// Seconds per step when replaying a game (at 1x).
    pub replay_step_seconds: f64,
    /// Set the terminal window title to the open game.
    pub terminal_title: bool,
    /// Hours after midnight that still count as the previous baseball day,
    /// so late West Coast games stay on the scoreboard.
    pub day_rollover_hour: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            favorites: Vec::new(),
            live_poll_seconds: 10,
            scoreboard_poll_seconds: 30,
            replay_step_seconds: 1.2,
            terminal_title: true,
            day_rollover_hour: 6,
        }
    }
}

const DEFAULT_FILE: &str = r#"# Ballatui settings

# Team abbreviations pinned to the top of the scoreboard and highlighted in
# the standings, e.g. ["SEA", "CHC"].
favorites = []

# Seconds between refreshes of a live game and of the scoreboard.
live_poll_seconds = 10
scoreboard_poll_seconds = 30

# Seconds per step when replaying a game at 1x speed.
replay_step_seconds = 1.2

# Show the open game in the terminal title.
terminal_title = true

# Hours after midnight that still count as the previous day's games.
day_rollover_hour = 6
"#;

pub fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "ballatui")
}

pub fn config_path() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().join("config.toml"))
}

pub fn load() -> Result<Config> {
    let Some(path) = config_path() else {
        return Ok(Config::default());
    };
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&path, DEFAULT_FILE).ok();
        return Ok(Config::default());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut cfg: Config =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    for f in &mut cfg.favorites {
        *f = f.trim().to_uppercase();
    }
    cfg.live_poll_seconds = cfg.live_poll_seconds.max(3);
    cfg.scoreboard_poll_seconds = cfg.scoreboard_poll_seconds.max(5);
    if cfg.replay_step_seconds.is_nan() || cfg.replay_step_seconds <= 0.05 {
        cfg.replay_step_seconds = 1.2;
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_parses() {
        let cfg: Config = toml::from_str(DEFAULT_FILE).unwrap();
        assert!(cfg.favorites.is_empty());
        assert_eq!(cfg.live_poll_seconds, 10);
    }
}
