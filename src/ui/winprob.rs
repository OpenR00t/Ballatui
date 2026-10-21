// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Win probability through the game, plate appearance by plate appearance,
//! with the plays that swung it most.

use super::gameview::Ctx;
use super::{pad, panel, rpad};
use crate::app::{GameView, SWINGS_SHOWN};
use crate::game::last_name;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CLine};
use ratatui::widgets::{Paragraph, Wrap};

const HOME: Color = theme::AQUA;
const AWAY: Color = theme::ORANGE;

/// FanGraphs' leverage buckets.
fn leverage_label(li: f64) -> (&'static str, Style) {
    if li >= 2.0 {
        (
            "high",
            Style::new().fg(theme::RED).add_modifier(Modifier::BOLD),
        )
    } else if li >= 0.85 {
        ("medium", theme::text())
    } else {
        ("low", theme::meta())
    }
}

/// A one-cell number chip for a swing: dark digit on the color of the team
/// that gained, or on light gray when selected. Plain digits keep it one
/// cell wide in every font.
fn rank_chip(i: usize, home_gained: bool, selected: bool) -> Span<'static> {
    let bg = if selected {
        theme::FG1
    } else if home_gained {
        HOME
    } else {
        AWAY
    };
    Span::styled(
        (i + 1).to_string(),
        Style::new()
            .fg(theme::BG0)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn draw(f: &mut Frame, ctx: &Ctx, g: &GameView, area: Rect) {
    let Some(wp) = g.wp.as_ref() else {
        let text = match &g.wp_error {
            Some(e) => format!("Couldn't load win probability: {e}"),
            None if ctx.feed.live_data.plays.all_plays.is_empty() => {
                "Win probability starts with the first plate appearance.".into()
            }
            None => "Loading win probability…".into(),
        };
        super::message(f, area, &text, theme::meta());
        return;
    };
    let chart_h = (area.height * 55 / 100).max(10);
    let [chart, bottom] =
        Layout::vertical([Constraint::Length(chart_h), Constraint::Min(4)]).areas(area);
    let swings = g.wp_swings();
    let top: Vec<_> = swings.iter().take(SWINGS_SHOWN).cloned().collect();
    draw_chart(f, ctx, g, wp, &top, chart);
    let [list, now] =
        Layout::horizontal([Constraint::Min(60), Constraint::Length(34)]).areas(bottom);
    draw_swings(f, ctx, g, &top, list);
    draw_now(f, ctx, g, now);
}

fn draw_chart(
    f: &mut Frame,
    ctx: &Ctx,
    g: &GameView,
    wp: &[crate::model::WpPoint],
    top: &[crate::model::WpPoint],
    area: Rect,
) {
    let gd = &ctx.feed.game_data;
    let plays = &ctx.feed.live_data.plays.all_plays;
    let block = panel(
        &format!(
            "Win probability · {} above, {} below",
            gd.teams.home.abbreviation, gd.teams.away.abbreviation
        ),
        false,
    );
    // Only plays up to the shown moment.
    let shown: Vec<(f64, f64)> = {
        let mut v = vec![(0.0, 50.0)];
        let limit = g.at().map(|c| c.play).unwrap_or(0);
        let at_end = g.at().is_some_and(|c| {
            plays
                .get(c.play)
                .is_some_and(|p| c.events >= p.play_events.len() && p.about.is_complete)
        });
        for w in wp {
            if w.at_bat_index < limit || (w.at_bat_index == limit && at_end) {
                v.push(((w.at_bat_index + 1) as f64, w.home_team_win_probability));
            }
        }
        v
    };
    let x_max = plays.len().max(wp.len()).max(1) as f64;
    // Where each inning starts, for the separators.
    let mut innings: Vec<(f64, u32, bool)> = Vec::new();
    let mut last = None;
    for (i, p) in plays.iter().enumerate() {
        let key = (p.about.inning, p.about.is_top_inning);
        if last != Some(key) {
            last = Some(key);
            innings.push((i as f64, p.about.inning, p.about.is_top_inning));
        }
    }
    // The cursor sits on the point after the shown play once it's done.
    let cursor_x = (!g.following())
        .then(|| {
            g.at().map(|c| {
                let done = plays
                    .get(c.play)
                    .is_some_and(|p| c.events >= p.play_events.len() && p.about.is_complete);
                c.play as f64 + if done { 1.0 } else { 0.5 }
            })
        })
        .flatten();
    // Every listed swing gets its chip on the line; the selected one last
    // so it's drawn on top.
    let mut marks: Vec<(f64, f64, usize, bool, bool)> = top
        .iter()
        .enumerate()
        .map(|(i, w)| {
            (
                (w.at_bat_index + 1) as f64,
                w.home_team_win_probability,
                i,
                w.home_team_win_probability_added >= 0.0,
                i == g.wp_sel,
            )
        })
        .collect();
    marks.sort_by_key(|m| m.4);
    let (home, away) = (
        gd.teams.home.abbreviation.clone(),
        gd.teams.away.abbreviation.clone(),
    );
    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .background_color(theme::BG0)
        .x_bounds([-x_max * 0.06, x_max])
        .y_bounds([-4.0, 104.0])
        .paint(move |c| {
            c.draw(&CLine::new(0.0, 50.0, x_max, 50.0, theme::BG3));
            for (x, _, top) in &innings {
                if *top {
                    c.draw(&CLine::new(*x, 0.0, *x, 100.0, theme::BG1));
                }
            }
            if let Some(x) = cursor_x {
                c.draw(&CLine::new(x, 0.0, x, 100.0, theme::FG4));
            }
            for w in shown.windows(2) {
                let (x0, y0) = w[0];
                let (x1, y1) = w[1];
                let color = if (y0 + y1) / 2.0 >= 50.0 { HOME } else { AWAY };
                c.draw(&CLine::new(x0, y0, x1, y1, color));
            }
            c.layer();
            c.print(
                -x_max * 0.06,
                100.0,
                Span::styled(
                    home.clone(),
                    Style::new().fg(HOME).add_modifier(Modifier::BOLD),
                ),
            );
            c.print(-x_max * 0.06, 50.0, Span::styled("50%", theme::meta()));
            c.print(
                -x_max * 0.06,
                2.0,
                Span::styled(
                    away.clone(),
                    Style::new().fg(AWAY).add_modifier(Modifier::BOLD),
                ),
            );
            for (x, inn, top) in &innings {
                if *top {
                    c.print(*x + 0.3, -4.0, Span::styled(inn.to_string(), theme::meta()));
                }
            }
            for (x, y, i, home_gained, sel) in &marks {
                c.print(*x, *y, rank_chip(*i, *home_gained, *sel));
            }
        });
    f.render_widget(canvas, area);
}

fn draw_swings(f: &mut Frame, ctx: &Ctx, g: &GameView, top: &[crate::model::WpPoint], area: Rect) {
    let block = panel("Biggest swings  ·  ⏎ show in gamecast", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let gd = &ctx.feed.game_data;
    let plays = &ctx.feed.live_data.plays.all_plays;
    let w = inner.width as usize;
    let mut lines = Vec::new();
    for (i, s) in top.iter().enumerate() {
        let y = inner.y + lines.len() as u16;
        if y < inner.y + inner.height {
            ctx.hit(
                Rect::new(inner.x, y, inner.width, 1),
                crate::app::Click::SwingRow(i),
            );
        }
        let Some(p) = plays.get(s.at_bat_index) else {
            continue;
        };
        let gain = s.home_team_win_probability_added;
        let (team, color) = if gain >= 0.0 {
            (&gd.teams.home.abbreviation, HOME)
        } else {
            (&gd.teams.away.abbreviation, AWAY)
        };
        let inn = format!(
            "{}{}",
            if p.about.is_top_inning { "▲" } else { "▼" },
            p.about.inning
        );
        let desc = p.result.description.clone().unwrap_or_default();
        let sel = i == g.wp_sel;
        let mut line = Line::from(vec![
            Span::styled(if sel { " ▶" } else { "  " }, theme::accent()),
            rank_chip(i, gain >= 0.0, sel),
            Span::raw(" "),
            Span::styled(pad(&inn, 4), theme::accent2()),
            Span::styled(
                rpad(&format!("{team} +{:.1}%", gain.abs()), 12),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                rpad(
                    &s.leverage_index
                        .map(|l| format!("LI {l:.2}"))
                        .unwrap_or_default(),
                    9,
                ),
                s.leverage_index
                    .map(|l| leverage_label(l).1)
                    .unwrap_or(theme::meta()),
            ),
            Span::raw("  "),
            Span::styled(pad(&desc, w.saturating_sub(35)), theme::text()),
        ]);
        if sel {
            line = line.style(theme::selected());
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(Line::styled("  No plays yet.", theme::meta()));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_now(f: &mut Frame, ctx: &Ctx, g: &GameView, area: Rect) {
    let block = panel("Now", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let gd = &ctx.feed.game_data;
    let mut lines = Vec::new();
    match g.wp_now() {
        Some(w) => {
            let home = w.home_team_win_probability;
            let (team, pct, color) = if home >= 50.0 {
                (&gd.teams.home.team_name, home, HOME)
            } else {
                (&gd.teams.away.team_name, 100.0 - home, AWAY)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {team} "),
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{pct:.1}%"), theme::title()),
                Span::styled(" to win", theme::meta()),
            ]));
            // A bar split between the teams.
            let bar_w = inner.width.saturating_sub(2) as usize;
            let home_cells = ((home / 100.0) * bar_w as f64).round() as usize;
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "█".repeat(bar_w - home_cells.min(bar_w)),
                    Style::new().fg(AWAY),
                ),
                Span::styled("█".repeat(home_cells.min(bar_w)), Style::new().fg(HOME)),
            ]));
            lines.push(Line::from(vec![
                Span::styled(
                    pad(
                        &format!(" {} {:.0}%", gd.teams.away.abbreviation, 100.0 - home),
                        bar_w / 2 + 1,
                    ),
                    Style::new().fg(AWAY),
                ),
                Span::styled(
                    rpad(
                        &format!("{} {:.0}%", gd.teams.home.abbreviation, home),
                        bar_w - bar_w / 2,
                    ),
                    Style::new().fg(HOME),
                ),
            ]));
            lines.push(Line::raw(""));
            let plays = &ctx.feed.live_data.plays.all_plays;
            if let Some(p) = plays.get(w.at_bat_index) {
                lines.push(Line::styled(" Last play", theme::accent2()));
                let who = last_name(ctx.feed, p.matchup.batter.id, &p.matchup.batter.full_name);
                let gain = w.home_team_win_probability_added;
                let team = if gain >= 0.0 {
                    &gd.teams.home.abbreviation
                } else {
                    &gd.teams.away.abbreviation
                };
                lines.push(Line::styled(
                    format!(
                        " {who}: {} {team} +{:.1}%",
                        p.result.event.clone().unwrap_or_default(),
                        gain.abs()
                    ),
                    theme::text(),
                ));
            }
            // Leverage of the next plate appearance (or of the last one).
            let next =
                g.wp.as_ref()
                    .and_then(|v| v.iter().find(|x| x.at_bat_index == w.at_bat_index + 1))
                    .and_then(|x| x.leverage_index)
                    .or(w.leverage_index);
            if let Some(li) = next {
                let (label, style) = leverage_label(li);
                lines.push(Line::from(vec![
                    Span::styled(" Leverage ", theme::meta()),
                    Span::styled(format!("{li:.2} "), theme::title()),
                    Span::styled(label, style),
                ]));
            }
        }
        None => lines.push(Line::styled(" 50% before the first pitch", theme::meta())),
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " MLB's model. Leverage (LI): how much the situation mattered; 1.0 average, 2+ high",
        theme::meta(),
    ));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
