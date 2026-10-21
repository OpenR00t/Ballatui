// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! The game screen frame: header, linescore and tab bar around the four
//! game tabs.

use super::{
    boxscore, gamecast, matchup, message, ordinal, pad, panel, pitches, plays, rpad, scorecard,
    spray,
};
use crate::app::{App, GameTab, GameView, SPEEDS};
use crate::game::{Cursor, Situation, total_steps};
use crate::model::GameFeed;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Runs per inning, `[away, home]`.
type InningRuns = [Option<u32>; 2];
/// Runs, hits, errors.
type Rhe = [Option<u32>; 3];

/// What every tab needs about the shown moment.
pub struct Ctx<'a> {
    pub feed: &'a GameFeed,
    pub cursor: Cursor,
    pub sit: Situation,
    /// Showing the latest state (not history or a replay).
    pub at_end: bool,
    pub favorites: &'a [String],
    pub stats: &'a crate::stats::StatsCache,
    /// Season of the game, for stat lookups.
    pub season: i32,
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let favorites = app.cfg.favorites.clone();
    let stats = &app.stats;
    let Some(g) = app.game.as_mut() else { return };
    let Some(feed) = g.feed.as_ref() else {
        let (text, style) = match &g.error {
            Some(e) => (format!("Couldn't load the game: {e}"), theme::error()),
            None => ("Loading game…".to_string(), theme::meta()),
        };
        message(f, area, &text, style);
        return;
    };

    let plays = &feed.live_data.plays.all_plays;
    let cursor = g.at();
    let sit = cursor.map(|c| Situation::at(plays, c)).unwrap_or_default();
    let ctx = Ctx {
        feed,
        cursor: cursor.unwrap_or(Cursor::start()),
        sit,
        at_end: g.following(),
        favorites: &favorites,
        stats,
        season: feed.game_data.game.season.parse().unwrap_or(0),
    };

    let [head, line, tabs, body] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Min(5),
    ])
    .areas(area);
    draw_header(f, g, &ctx, head);
    draw_linescore(f, &ctx, cursor.is_some(), line);
    draw_tab_bar(f, g.tab, tabs);
    match g.tab {
        GameTab::Gamecast => gamecast::draw(f, &ctx, g.scroll, body),
        GameTab::Matchup => matchup::draw(f, &ctx, body),
        GameTab::Spray => spray::draw(f, &ctx, g, body),
        GameTab::Pitches => pitches::draw(f, &ctx, g, body),
        GameTab::Box => boxscore::draw(f, &ctx, g.home_side, g.scroll, body),
        GameTab::Scorecard => scorecard::draw(f, &ctx, g.home_side, g.scroll, g.hscroll, body),
        GameTab::Plays => {
            let (len, _) = plays::draw(f, &ctx, g.plays_sel, body);
            g.plays_len = len;
            if g.plays_sel >= len {
                g.plays_sel = len.saturating_sub(1);
            }
        }
    }
}

fn draw_header(f: &mut Frame, g: &GameView, ctx: &Ctx, area: Rect) {
    let gd = &ctx.feed.game_data;
    let (away, home) = (&gd.teams.away, &gd.teams.home);
    let runs = if ctx.at_end && !gd.status.is_preview() {
        let t = &ctx.feed.live_data.linescore.teams;
        [t.away.runs.unwrap_or(0), t.home.runs.unwrap_or(0)]
    } else {
        ctx.sit.runs
    };
    let show_score = !gd.status.is_preview() || !ctx.feed.live_data.plays.all_plays.is_empty();
    let team = |t: &crate::model::TeamInfo| {
        let fav = ctx.favorites.iter().any(|f| f == &t.abbreviation);
        Span::styled(
            t.name.clone(),
            if fav {
                theme::accent_bold()
            } else {
                theme::title()
            },
        )
    };
    let mut l1 = vec![Span::raw(" "), team(away)];
    if show_score {
        l1.push(Span::styled(format!("  {}", runs[0]), theme::title()));
    }
    l1.push(Span::styled("  @  ", theme::meta()));
    l1.push(team(home));
    if show_score {
        l1.push(Span::styled(format!("  {}", runs[1]), theme::title()));
    }
    let mut info = Vec::new();
    if let Some(n) = &gd.venue.name {
        info.push(n.clone());
    }
    if let Some(w) = &gd.weather
        && let (Some(t), Some(c)) = (&w.temp, &w.condition)
    {
        info.push(format!("{t}°F {c}"));
    }
    l1.push(Span::styled(
        format!("     {}", info.join(" · ")),
        theme::meta(),
    ));

    // Second line: what is being shown.
    let plays = &ctx.feed.live_data.plays.all_plays;
    let mut l2 = vec![Span::raw(" ")];
    if !g.following() {
        let pos = format!(
            "  step {}/{}",
            ctx.cursor.ordinal(plays),
            total_steps(plays)
        );
        if g.replay.playing {
            l2.push(Span::styled(
                format!("▶ REPLAY {}x", SPEEDS[g.replay.speed_idx]),
                theme::accent_bold(),
            ));
        } else {
            l2.push(Span::styled(
                "◀ HISTORY",
                theme::accent2().add_modifier(Modifier::BOLD),
            ));
        }
        l2.push(Span::styled(pos, theme::meta()));
        l2.push(Span::styled(
            format!(
                "  ·  {} {}",
                if ctx.sit.top { "Top" } else { "Bottom" },
                ordinal(ctx.sit.inning)
            ),
            theme::secondary(),
        ));
        l2.push(Span::styled("   f ", theme::key()));
        l2.push(Span::styled(
            if gd.status.is_live() {
                "back to live"
            } else {
                "end of game"
            },
            theme::meta(),
        ));
    } else if gd.status.is_live() {
        l2.push(Span::styled("● LIVE", theme::accent_bold()));
        let ls = &ctx.feed.live_data.linescore;
        if let (Some(state), Some(inn)) = (&ls.inning_state, ls.current_inning) {
            l2.push(Span::styled(
                format!("  {state} {}", ordinal(inn)),
                theme::text(),
            ));
        }
        if gd.status.detailed_state != "In Progress" {
            l2.push(Span::styled(
                format!("  ·  {}", gd.status.detailed_state),
                theme::warn(),
            ));
        }
        if let Some(t) = g.last_fetch {
            l2.push(Span::styled(
                format!("  ·  updated {}s ago", t.elapsed().as_secs()),
                theme::meta(),
            ));
        }
    } else if gd.status.is_final() {
        let ls = &ctx.feed.live_data.linescore;
        let inn = ls.current_inning.unwrap_or(9);
        let sched = ls.scheduled_innings.unwrap_or(9);
        let text = if inn != sched {
            format!("{} ({inn} innings)", gd.status.detailed_state)
        } else {
            gd.status.detailed_state.clone()
        };
        l2.push(Span::styled(
            text,
            theme::secondary().add_modifier(Modifier::BOLD),
        ));
        if let Some(d) = &ctx.feed.live_data.decisions {
            let mut parts = Vec::new();
            if let Some(w) = &d.winner {
                parts.push(format!("W {}", w.full_name));
            }
            if let Some(l) = &d.loser {
                parts.push(format!("L {}", l.full_name));
            }
            if let Some(s) = &d.save {
                parts.push(format!("S {}", s.full_name));
            }
            l2.push(Span::styled(
                format!("  ·  {}", parts.join("  ")),
                theme::meta(),
            ));
        }
        l2.push(Span::styled("   ␣ ", theme::key()));
        l2.push(Span::styled("replay", theme::meta()));
    } else {
        let st = &gd.status;
        let style = if st.detailed_state.starts_with("Delayed") || st.detailed_state == "Postponed"
        {
            theme::warn()
        } else {
            theme::text()
        };
        l2.push(Span::styled(
            format!(
                "{}  ·  first pitch {}",
                st.detailed_state,
                super::local_time(&gd.datetime.date_time)
            ),
            style,
        ));
    }
    if g.loading {
        l2.push(Span::styled("  ⟳", theme::key()));
    }
    if let Some(e) = &g.error {
        l2.push(Span::styled(
            format!("  refresh failed: {e}"),
            theme::error(),
        ));
    }
    f.render_widget(Paragraph::new(vec![Line::from(l1), Line::from(l2)]), area);
}

fn draw_linescore(f: &mut Frame, ctx: &Ctx, has_plays: bool, area: Rect) {
    let block = panel("Linescore", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let gd = &ctx.feed.game_data;
    let ls = &ctx.feed.live_data.linescore;
    let sched = ls.scheduled_innings.unwrap_or(9);
    let (line, rhe): (Vec<InningRuns>, [Rhe; 2]) = if ctx.at_end || !has_plays {
        (
            ls.innings
                .iter()
                .map(|i| [i.away.runs, i.home.runs])
                .collect(),
            [
                [ls.teams.away.runs, ls.teams.away.hits, ls.teams.away.errors],
                [ls.teams.home.runs, ls.teams.home.hits, ls.teams.home.errors],
            ],
        )
    } else {
        let s = &ctx.sit;
        (
            s.line.clone(),
            [
                [Some(s.runs[0]), Some(s.hits[0]), Some(s.errors[0])],
                [Some(s.runs[1]), Some(s.hits[1]), Some(s.errors[1])],
            ],
        )
    };
    let current = (gd.status.is_live() || !ctx.at_end) && has_plays;
    let fav = [
        ctx.favorites
            .iter()
            .any(|f| f == &gd.teams.away.abbreviation),
        ctx.favorites
            .iter()
            .any(|f| f == &gd.teams.home.abbreviation),
    ];
    let mut lines = linescore_lines(
        [&gd.teams.away.abbreviation, &gd.teams.home.abbreviation],
        &line,
        rhe,
        sched,
        inner.width as usize,
        fav,
        gd.status.is_final() && ctx.at_end,
    );
    if current {
        // Mark the half-inning being shown.
        let row = if ctx.sit.top { 1 } else { 2 };
        let first = first_inning(line.len().max(sched as usize), inner.width as usize);
        let col = ctx.sit.inning as usize;
        if col > first {
            let span_idx = 1 + (col - first - 1);
            if let Some(span) = lines[row].spans.get_mut(span_idx) {
                span.style = span.style.bg(theme::BG1).add_modifier(Modifier::BOLD);
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// First inning number shown (0-based offset) when space is short.
fn first_inning(innings: usize, width: usize) -> usize {
    let fixed = 6 + 3 * 4;
    let fit = width.saturating_sub(fixed) / 3;
    innings.saturating_sub(fit.max(1))
}

/// Three lines: inning numbers, away row, home row. Each inning cell is a
/// separate span so the current half can be highlighted.
pub fn linescore_lines(
    names: [&str; 2],
    line: &[[Option<u32>; 2]],
    rhe: [[Option<u32>; 3]; 2],
    scheduled: u32,
    width: usize,
    fav: [bool; 2],
    finished: bool,
) -> Vec<Line<'static>> {
    let innings = line.len().max(scheduled as usize);
    let first = first_inning(innings, width);
    let mut head = vec![Span::styled(pad("", 6), theme::meta())];
    for i in first..innings {
        head.push(Span::styled(rpad(&(i + 1).to_string(), 3), theme::meta()));
    }
    head.push(Span::raw("   "));
    for h in ["R", "H", "E"] {
        head.push(Span::styled(rpad(h, 3), theme::accent()));
    }
    let mut out = vec![Line::from(head)];
    for t in 0..2 {
        let name_style = if fav[t] {
            theme::accent_bold()
        } else {
            theme::title()
        };
        let mut spans = vec![Span::styled(pad(&format!(" {}", names[t]), 6), name_style)];
        for i in first..innings {
            let v = line.get(i).and_then(|r| r[t]);
            let text = match v {
                Some(n) => n.to_string(),
                // A home team that didn't need its last half.
                None if finished
                    && t == 1
                    && i + 1 == line.len()
                    && i + 1 >= scheduled as usize
                    && rhe[1][0] > rhe[0][0] =>
                {
                    "x".into()
                }
                None => String::new(),
            };
            let style = if v.unwrap_or(0) > 0 {
                theme::title()
            } else {
                theme::secondary()
            };
            spans.push(Span::styled(rpad(&text, 3), style));
        }
        spans.push(Span::raw("   "));
        for (k, v) in rhe[t].iter().enumerate() {
            let s = if k == 0 {
                theme::title()
            } else {
                theme::text()
            };
            spans.push(Span::styled(
                rpad(&v.map(|n| n.to_string()).unwrap_or_default(), 3),
                s,
            ));
        }
        out.push(Line::from(spans));
    }
    out
}

fn draw_tab_bar(f: &mut Frame, tab: GameTab, area: Rect) {
    let mut spans = vec![Span::raw(" ")];
    for t in GameTab::ALL {
        let active = t == tab;
        spans.push(Span::styled(format!("{} ", t.key()), theme::key()));
        spans.push(Span::styled(
            t.label(),
            if active {
                theme::accent_bold().add_modifier(Modifier::UNDERLINED)
            } else {
                theme::secondary()
            },
        ));
        spans.push(Span::styled("   ", Style::new()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
