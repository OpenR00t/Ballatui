// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Ballatui: live baseball in the terminal.

mod api;
mod app;
mod config;
mod game;
mod model;
mod stats;
mod team;
mod theme;
mod ui;

use anyhow::{Context, Result, bail};
use app::{App, Screen};
use chrono::NaiveDate;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::SetTitle;
use std::time::Duration;

const HELP: &str = "\
ballatui - live baseball in the terminal

USAGE:
    ballatui [OPTIONS]

OPTIONS:
    -d, --date <YYYY-MM-DD>   open the scoreboard on this date
    -g, --game <GAME_PK>      open this game directly
        --paths               print where the settings file lives
    -h, --help                print this help
    -V, --version             print the version

Press ? inside the app for keys.
Data: MLB Stats API (statsapi.mlb.com), for personal, non-commercial use.";

struct Args {
    date: Option<NaiveDate>,
    game: Option<u64>,
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = Args {
        date: None,
        game: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("ballatui {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--paths" | "paths" => {
                match config::config_path() {
                    Some(p) => println!("config: {}", p.display()),
                    None => println!("no config directory on this system"),
                }
                return Ok(None);
            }
            "-d" | "--date" => {
                let v = it.next().context("--date needs a value")?;
                args.date = Some(
                    NaiveDate::parse_from_str(&v, "%Y-%m-%d")
                        .with_context(|| format!("bad date {v:?}, expected YYYY-MM-DD"))?,
                );
            }
            "-g" | "--game" => {
                let v = it.next().context("--game needs a value")?;
                args.game = Some(v.parse().with_context(|| format!("bad game id {v:?}"))?);
            }
            other => bail!("unknown argument {other:?} (try --help)"),
        }
    }
    Ok(Some(args))
}

fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let cfg = config::load()?;
    let api = api::Api::new()?;
    let mut app = App::new(cfg, api, args.date, args.game);

    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture).ok();
    let result = run(&mut terminal, &mut app);
    execute!(std::io::stdout(), DisableMouseCapture).ok();
    if app.cfg.terminal_title {
        execute!(std::io::stdout(), SetTitle("")).ok();
    }
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.quit {
        app.handle_msgs();
        app.tick();
        if app.cfg.terminal_title {
            let title = window_title(app);
            if title != app.last_title {
                execute!(std::io::stdout(), SetTitle(&title)).ok();
                app.last_title = title;
            }
        }
        terminal.draw(|f| ui::draw(f, app))?;
        let wait = if app.wants_fast_ticks() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(250)
        };
        if event::poll(wait)? {
            match event::read()? {
                // Windows also reports key releases; act on presses only.
                Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                Event::Mouse(m) => app.on_mouse(m),
                _ => {}
            }
        }
    }
    Ok(())
}

fn window_title(app: &App) -> String {
    if app.screen == Screen::Game
        && let Some(f) = app.game.as_ref().and_then(|g| g.feed.as_ref())
    {
        let ls = &f.live_data.linescore;
        let (a, h) = (
            &f.game_data.teams.away.abbreviation,
            &f.game_data.teams.home.abbreviation,
        );
        if f.game_data.status.is_preview() {
            return format!("Ballatui · {a} @ {h}");
        }
        let mut t = format!(
            "Ballatui · {a} {} @ {h} {}",
            ls.teams.away.runs.unwrap_or(0),
            ls.teams.home.runs.unwrap_or(0)
        );
        if f.game_data.status.is_live() {
            if let (Some(half), Some(inn)) = (&ls.inning_half, ls.current_inning) {
                let arrow = if half == "Top" { "▲" } else { "▼" };
                t.push_str(&format!(" {arrow}{inn}"));
            }
        } else if f.game_data.status.is_final() {
            t.push_str(" F");
        }
        return t;
    }
    "Ballatui".into()
}
