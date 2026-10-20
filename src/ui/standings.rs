// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

use super::{message, pad, panel, rpad};
use crate::app::{App, StandingsKind};
use crate::model::{StandingsRecord, TeamRecord, division_name, league_name};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const COLS: [(&str, usize); 7] = [
    ("W", 4),
    ("L", 4),
    ("PCT", 6),
    ("GB", 6),
    ("L10", 6),
    ("STRK", 6),
    ("DIFF", 6),
];
const TEAM_W: usize = 18;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let kind = app.standings_kind;
    let title = match kind {
        StandingsKind::Division => "Standings · divisions  (w: wild card)",
        StandingsKind::WildCard => "Standings · wild card  (w: divisions)",
    };
    let block = panel(title, true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(st) = app.standings.get(&kind) else {
        let (text, style) = match &app.standings_error {
            Some(e) => (format!("Couldn't load standings: {e}"), theme::error()),
            None => ("Loading standings…".to_string(), theme::meta()),
        };
        message(f, inner, &text, style);
        return;
    };
    let mut leagues: Vec<(u64, Vec<&StandingsRecord>)> = Vec::new();
    for lg in [103u64, 104] {
        let mut recs: Vec<&StandingsRecord> =
            st.records.iter().filter(|r| r.league.id == lg).collect();
        recs.sort_by_key(|r| match r.division.as_ref().map(|d| d.id) {
            Some(201) | Some(204) => 0,
            Some(202) | Some(205) => 1,
            _ => 2,
        });
        leagues.push((lg, recs));
    }
    let table_w = (TEAM_W + COLS.iter().map(|c| c.1).sum::<usize>() + 2) as u16;
    let side_by_side = inner.width >= table_w * 2 + 4;
    let favs = &app.cfg.favorites;
    let render = |lg: u64, recs: &[&StandingsRecord]| -> Vec<Line<'static>> {
        let mut lines = vec![Line::styled(
            format!(" {}", league_name(lg)),
            theme::title(),
        )];
        for r in recs {
            lines.push(Line::raw(""));
            let name = match kind {
                StandingsKind::Division => r
                    .division
                    .as_ref()
                    .map(|d| division_name(d.id))
                    .unwrap_or("Division")
                    .to_string(),
                StandingsKind::WildCard => "Wild card".to_string(),
            };
            let mut head = vec![Span::styled(
                pad(&format!(" {name}"), TEAM_W),
                theme::accent_bold(),
            )];
            for (c, w) in COLS {
                let c = if kind == StandingsKind::WildCard && c == "GB" {
                    "WCGB"
                } else {
                    c
                };
                head.push(Span::styled(rpad(c, w), theme::accent()));
            }
            lines.push(Line::from(head));
            for (i, t) in r.team_records.iter().enumerate() {
                lines.push(team_line(t, kind, favs));
                if kind == StandingsKind::WildCard && i == 2 {
                    lines.push(Line::styled(
                        format!(" {}", "┄".repeat(table_w as usize - 2)),
                        Style::new().fg(theme::BG3),
                    ));
                }
            }
        }
        lines
    };
    let left = render(leagues[0].0, &leagues[0].1);
    let right = render(leagues[1].0, &leagues[1].1);
    let mut footer = vec![Line::raw("")];
    // Legend for the clinch letters after team names (not keys).
    let mut legend = vec![Span::styled(" Clinched: ", theme::meta())];
    for (i, (letter, meaning)) in [
        ("x", "playoff spot"),
        ("y", "division"),
        ("z", "best record in league"),
        ("w", "wild card"),
        ("e", "eliminated"),
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            legend.push(Span::styled(" · ", theme::meta()));
        }
        legend.push(Span::styled(*letter, clinch_style(letter)));
        legend.push(Span::styled(format!(" = {meaning}"), theme::meta()));
    }
    footer.push(Line::from(legend));
    if side_by_side {
        let [tables, foot] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(footer.len() as u16)])
                .areas(inner);
        let [a, b] =
            Layout::horizontal([Constraint::Length(table_w + 2), Constraint::Min(table_w)])
                .areas(tables);
        let max = left.len().max(right.len()) as u16;
        app.standings_scroll = app.standings_scroll.min(max.saturating_sub(tables.height));
        let s = app.standings_scroll;
        f.render_widget(Paragraph::new(left).scroll((s, 0)), a);
        f.render_widget(Paragraph::new(right).scroll((s, 0)), b);
        f.render_widget(Paragraph::new(footer), foot);
    } else {
        let mut all = left;
        all.push(Line::raw(""));
        all.extend(right);
        all.extend(footer);
        app.standings_scroll = app
            .standings_scroll
            .min((all.len() as u16).saturating_sub(inner.height));
        f.render_widget(Paragraph::new(all).scroll((app.standings_scroll, 0)), inner);
    }
}

fn team_line(t: &TeamRecord, kind: StandingsKind, favs: &[String]) -> Line<'static> {
    let fav = favs.iter().any(|f| f == &t.team.abbreviation);
    let clinch = t.clinch_indicator.clone().unwrap_or_default();
    let name = format!(" {}", t.team.team_name);
    let name_style = if fav {
        theme::accent_bold()
    } else if clinch == "e" {
        theme::secondary()
    } else {
        theme::text()
    };
    let l10 = t
        .split("lastTen")
        .map(|s| format!("{}-{}", s.wins, s.losses))
        .unwrap_or_default();
    let gb = match kind {
        StandingsKind::Division => t.games_back.clone(),
        StandingsKind::WildCard => t.wild_card_games_back.clone().unwrap_or_default(),
    };
    let diff = if t.run_differential > 0 {
        format!("+{}", t.run_differential)
    } else {
        t.run_differential.to_string()
    };
    let diff_style = if t.run_differential > 0 {
        theme::accent()
    } else if t.run_differential < 0 {
        theme::error()
    } else {
        theme::meta()
    };
    let vals = [
        t.wins.to_string(),
        t.losses.to_string(),
        t.winning_percentage.clone(),
        gb,
        l10,
        t.streak
            .as_ref()
            .map(|s| s.streak_code.clone())
            .unwrap_or_default(),
    ];
    let name_w = TEAM_W.saturating_sub(clinch.chars().count() + 1);
    let mut spans = vec![
        Span::styled(pad(&name, name_w), name_style),
        Span::styled(format!("{clinch} "), clinch_style(&clinch)),
    ];
    if clinch.is_empty() {
        spans[1] = Span::raw(" ");
    }
    for (v, (_, w)) in vals.iter().zip(COLS) {
        spans.push(Span::styled(rpad(v, w), theme::text()));
    }
    spans.push(Span::styled(rpad(&diff, COLS[6].1), diff_style));
    let mut line = Line::from(spans);
    if fav {
        line = line.style(theme::selected());
    }
    line
}

/// Clinch letters are tags, not keys: aqua, with eliminated dimmed.
fn clinch_style(letter: &str) -> Style {
    if letter == "e" {
        theme::meta()
    } else {
        theme::accent2()
    }
}
