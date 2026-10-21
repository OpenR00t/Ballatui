// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Team page: record and standing, schedule around today (open any game),
//! team stats ranked against the league, leaders, and the roster.

use super::{centered, local_time, message, ordinal, pad, panel, rpad};
use crate::app::{App, Click, StandingsKind, TeamTab};
use crate::model::{ScheduleGame, TeamRecord, division_name};
use crate::stats::{Load, text};
use crate::team::{Ranked, RosterPlayer, TeamPage};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if app.team_sel == usize::MAX && app.team_id.is_some() && !app.team_games().is_empty() {
        app.team_sel = app.team_default_sel();
    }
    let app = &*app;
    match app.team_id {
        None => {
            let text = if app.teams.is_none() {
                "Loading teams…"
            } else {
                "Pick a team with t"
            };
            message(f, area, text, theme::meta());
        }
        Some(id) => {
            let team = app
                .teams
                .as_ref()
                .and_then(|t| t.iter().find(|t| t.id == id).cloned())
                .unwrap_or_default();
            let record = app.standings.get(&StandingsKind::Division).and_then(|s| {
                s.records
                    .iter()
                    .flat_map(|r| r.team_records.iter())
                    .find(|r| r.team.id == id)
                    .cloned()
            });
            let [head, tabs, body] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(5),
            ])
            .areas(area);
            let page = app.team_pages.get(&id);
            draw_header(
                f,
                app,
                &team,
                record.as_ref(),
                page.and_then(|p| p.ready()),
                head,
            );
            draw_tab_bar(f, app, app.team_tab, tabs);
            match page {
                Some(Load::Ready(p)) => match app.team_tab {
                    TeamTab::Overview => draw_overview(f, app, p, body),
                    TeamTab::Roster => draw_roster(f, app, p, body),
                },
                Some(Load::Failed(e)) => message(
                    f,
                    body,
                    &format!("Couldn't load the team: {e}"),
                    theme::error(),
                ),
                _ => message(f, body, "Loading team…", theme::meta()),
            }
        }
    }
    if let Some(sel) = app.picker {
        draw_picker(f, app, sel, area);
    }
}

fn draw_header(
    f: &mut Frame,
    app: &App,
    team: &crate::model::TeamInfo,
    rec: Option<&TeamRecord>,
    page: Option<&TeamPage>,
    area: Rect,
) {
    let fav = app.cfg.favorites.iter().any(|f| f == &team.abbreviation);
    let div = team
        .division
        .as_ref()
        .map(|d| division_name(d.id))
        .unwrap_or("");
    let mut l1 = vec![
        Span::styled(
            format!(" {}", team.name),
            if fav {
                theme::accent_bold()
            } else {
                theme::title()
            },
        ),
        Span::styled(
            format!("  ·  {div}  ·  {} season", app.team_season()),
            theme::meta(),
        ),
    ];
    if fav {
        l1.push(Span::styled("  ★", theme::accent()));
    }
    let mut l2 = vec![Span::raw(" ")];
    if let Some(r) = rec {
        let place = r
            .division_rank
            .as_deref()
            .and_then(|x| x.parse::<u32>().ok())
            .map(ordinal)
            .unwrap_or_default();
        let gb = if r.games_back == "-" {
            "leads".to_string()
        } else {
            format!("{} GB", r.games_back)
        };
        l2.push(Span::styled(
            format!("{}-{} ({})", r.wins, r.losses, r.winning_percentage),
            theme::title(),
        ));
        l2.push(Span::styled(
            format!("  ·  {place} in {div}, {gb}"),
            theme::text(),
        ));
        if let Some(c) = &r.clinch_indicator {
            let what = match c.as_str() {
                "z" => "best record in league",
                "y" => "won division",
                "w" => "wild card",
                "x" => "playoff spot",
                "e" => "eliminated",
                _ => "",
            };
            if !what.is_empty() {
                l2.push(Span::styled(format!("  ·  {what}"), theme::accent2()));
            }
        }
        if let Some(s) = &r.streak {
            l2.push(Span::styled(
                format!("  ·  streak {}", s.streak_code),
                theme::secondary(),
            ));
        }
        if let Some(t) = r.split("lastTen") {
            l2.push(Span::styled(
                format!("  ·  last 10 {}-{}", t.wins, t.losses),
                theme::secondary(),
            ));
        }
        let d = r.run_differential;
        l2.push(Span::styled(
            format!("  ·  run diff {}{d}", if d > 0 { "+" } else { "" }),
            if d > 0 {
                theme::accent()
            } else if d < 0 {
                theme::error()
            } else {
                theme::meta()
            },
        ));
    } else {
        l2.push(Span::styled("standings loading…", theme::meta()));
    }
    // Postseason: the latest series result in the schedule window.
    let mut l3 = vec![Span::raw(" ")];
    if let Some(g) = page.and_then(|p| {
        p.games.iter().rev().find(|g| {
            matches!(g.game_type.as_str(), "F" | "D" | "L" | "W") && g.series_status.is_some()
        })
    }) {
        let result = g
            .series_status
            .as_ref()
            .and_then(|s| s.result.clone())
            .unwrap_or_default();
        l3.push(Span::styled("Postseason  ", theme::meta()));
        l3.push(Span::styled(
            format!(
                "{} · {result}",
                g.series_description.clone().unwrap_or_default()
            ),
            theme::accent2(),
        ));
    }
    f.render_widget(
        Paragraph::new(vec![Line::from(l1), Line::from(l2), Line::from(l3)]),
        area,
    );
}

fn draw_tab_bar(f: &mut Frame, app: &App, tab: TeamTab, area: Rect) {
    app.hit(
        Rect::new(area.x + 1, area.y, 10, 1).intersection(area),
        Click::TeamTab(TeamTab::Overview),
    );
    app.hit(
        Rect::new(area.x + 14, area.y, 8, 1).intersection(area),
        Click::TeamTab(TeamTab::Roster),
    );
    let item = |k: &str, label: &str, active: bool| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!("{k} "), theme::key()),
            Span::styled(
                label.to_string(),
                if active {
                    theme::accent_bold().add_modifier(Modifier::UNDERLINED)
                } else {
                    theme::secondary()
                },
            ),
            Span::raw("   "),
        ]
    };
    let mut spans = vec![Span::raw(" ")];
    spans.extend(item("o", "Overview", tab == TeamTab::Overview));
    spans.extend(item("R", "Roster", tab == TeamTab::Roster));
    spans.push(Span::styled("t ", theme::key()));
    spans.push(Span::styled("pick team   ", theme::meta()));
    spans.push(Span::styled("f ", theme::key()));
    spans.push(Span::styled("next favorite", theme::meta()));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_overview(f: &mut Frame, app: &App, page: &TeamPage, area: Rect) {
    // Narrow terminals keep the schedule and drop leaders, then ranks.
    let w = area.width;
    if w >= 132 {
        let [sched, ranks, leaders] = Layout::horizontal([
            Constraint::Min(56),
            Constraint::Length(34),
            Constraint::Length(42),
        ])
        .areas(area);
        draw_schedule(f, app, &page.games, sched);
        draw_ranks(f, page, ranks);
        draw_leaders(f, page, leaders);
    } else if w >= 92 {
        let [sched, ranks] =
            Layout::horizontal([Constraint::Min(56), Constraint::Length(34)]).areas(area);
        draw_schedule(f, app, &page.games, sched);
        draw_ranks(f, page, ranks);
    } else {
        draw_schedule(f, app, &page.games, area);
    }
}

fn short_date(g: &ScheduleGame) -> String {
    chrono::NaiveDate::parse_from_str(&g.official_date, "%Y-%m-%d")
        .map(|d| d.format("%a %b %-d").to_string())
        .unwrap_or_default()
}

fn series_tag(g: &ScheduleGame) -> String {
    let short = match g.game_type.as_str() {
        "F" => "WC",
        "D" => "DS",
        "L" => "CS",
        "W" => "WS",
        "S" => "ST",
        _ => "",
    };
    match (short, g.series_game_number) {
        ("", _) => String::new(),
        ("ST", _) => "spring".into(),
        (s, Some(n)) => format!("{s} {n}"),
        (s, None) => s.into(),
    }
}

fn last(full: &str) -> String {
    match full.split_once(' ') {
        Some((_, rest)) if !rest.is_empty() => rest.to_string(),
        _ => full.to_string(),
    }
}

fn draw_schedule(f: &mut Frame, app: &App, games: &[ScheduleGame], area: Rect) {
    let block = panel("Schedule  ·  ⏎ open game", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(id) = app.team_id else { return };
    if games.is_empty() {
        message(f, inner, "No games within 45 days", theme::meta());
        return;
    }
    let today = app.date.format("%Y-%m-%d").to_string();
    let w = inner.width as usize;
    let mut rows: Vec<Line> = Vec::new();
    // Which game each row shows (the "today" marker shows none).
    let mut row_game: Vec<Option<usize>> = Vec::new();
    let mut sel_row = 0;
    let mut marked_today = false;
    for (i, g) in games.iter().enumerate() {
        if !marked_today && g.official_date >= today {
            marked_today = true;
            rows.push(Line::styled(
                format!(" {}", "┄".repeat(w.saturating_sub(2))),
                Style::new().fg(theme::BG3),
            ));
            row_game.push(None);
        }
        let home = g.teams.home.team.id == id;
        let (us, them) = if home {
            (&g.teams.home, &g.teams.away)
        } else {
            (&g.teams.away, &g.teams.home)
        };
        let opp = format!(
            "{} {}",
            if home { "vs" } else { " @" },
            them.team.abbreviation
        );
        let (result, rstyle, detail) = if g.status.is_final() {
            let (a, b) = (us.score.unwrap_or(0), them.score.unwrap_or(0));
            let won = us.is_winner == Some(true);
            let res = format!("{} {a}-{b}", if won { "W" } else { "L" });
            let d = g
                .decisions
                .as_ref()
                .map(|d| {
                    let mut v = Vec::new();
                    if let Some(w) = &d.winner {
                        v.push(format!("W {}", last(&w.full_name)));
                    }
                    if let Some(l) = &d.loser {
                        v.push(format!("L {}", last(&l.full_name)));
                    }
                    v.join("  ")
                })
                .unwrap_or_default();
            (
                res,
                if won {
                    theme::accent_bold()
                } else {
                    theme::error()
                },
                d,
            )
        } else if g.status.is_live() {
            (
                "LIVE".to_string(),
                theme::accent_bold(),
                g.status.detailed_state.clone(),
            )
        } else if matches!(g.status.detailed_state.as_str(), "Postponed" | "Cancelled") {
            (
                g.status.detailed_state.clone(),
                theme::warn(),
                String::new(),
            )
        } else {
            let a = us.probable_pitcher.as_ref().map(|p| last(&p.full_name));
            let b = them.probable_pitcher.as_ref().map(|p| last(&p.full_name));
            let d = if a.is_some() || b.is_some() {
                format!(
                    "{} vs {}",
                    a.unwrap_or_else(|| "TBD".into()),
                    b.unwrap_or_else(|| "TBD".into())
                )
            } else {
                String::new()
            };
            (local_time(&g.game_date), theme::text(), d)
        };
        let sel = i == app.team_sel;
        if sel {
            sel_row = rows.len();
        }
        let mut line = Line::from(vec![
            Span::styled(if sel { " ▶ " } else { "   " }, theme::accent()),
            Span::styled(pad(&short_date(g), 11), theme::secondary()),
            Span::styled(pad(&opp, 7), theme::text()),
            Span::styled(pad(&result, 9), rstyle),
            Span::styled(pad(&series_tag(g), 7), theme::accent2()),
            Span::styled(pad(&detail, w.saturating_sub(37)), theme::meta()),
        ]);
        if sel {
            line = line.style(theme::selected());
        }
        rows.push(line);
        row_game.push(Some(i));
    }
    let h = inner.height as usize;
    let first = super::keep_in_view(app.team_top.get(), sel_row, sel_row + 1, h)
        .min(rows.len().saturating_sub(h));
    app.team_top.set(first);
    for (n, g) in row_game.iter().enumerate().skip(first).take(h) {
        if let Some(i) = g {
            app.hit(
                Rect::new(inner.x, inner.y + (n - first) as u16, inner.width, 1),
                Click::TeamGame(*i),
            );
        }
    }
    f.render_widget(Paragraph::new(rows).scroll((first as u16, 0)), inner);
}

fn rank_style(r: &Ranked) -> Style {
    if r.rank <= 10 {
        theme::accent()
    } else if r.rank + 10 > r.of {
        theme::error()
    } else {
        theme::text()
    }
}

fn draw_ranks(f: &mut Frame, page: &TeamPage, area: Rect) {
    let mut lines = Vec::new();
    for (title, list) in [
        ("Hitting", &page.hitting_ranks),
        ("Pitching", &page.pitching_ranks),
    ] {
        lines.push(Line::styled(
            format!(" {title}"),
            theme::accent2().add_modifier(Modifier::BOLD),
        ));
        for r in list {
            lines.push(Line::from(vec![
                Span::styled(pad(&format!("  {}", r.label), 15), theme::secondary()),
                Span::styled(rpad(&r.value, 7), theme::title()),
                Span::styled(rpad(&ordinal(r.rank as u32), 7), rank_style(r)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::styled(" Rank among MLB teams;", theme::meta()));
    lines.push(Line::styled(" green top 10, red bottom 10", theme::meta()));
    f.render_widget(Paragraph::new(lines).block(panel("MLB rank", false)), area);
}

fn draw_leaders(f: &mut Frame, page: &TeamPage, area: Rect) {
    let w = area.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    for (label, list) in &page.leaders {
        let mut spans = vec![Span::styled(pad(&format!(" {label}"), 5), theme::accent2())];
        let mut used = 5;
        for (i, (name, value)) in list.iter().enumerate() {
            let piece = format!(" {} {value} ", last(name));
            if used + piece.chars().count() > w {
                break;
            }
            used += piece.chars().count();
            spans.push(Span::styled(
                format!(" {}", last(name)),
                if i == 0 {
                    theme::title()
                } else {
                    theme::secondary()
                },
            ));
            spans.push(Span::styled(
                format!(" {value} "),
                if i == 0 {
                    theme::accent_bold()
                } else {
                    theme::meta()
                },
            ));
        }
        if list.is_empty() {
            spans.push(Span::styled(" —", theme::meta()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " Regular season; AVG, OPS and ERA",
        theme::meta(),
    ));
    lines.push(Line::styled(" need qualifying playing time", theme::meta()));
    f.render_widget(Paragraph::new(lines).block(panel("Leaders", false)), area);
}

const POS_ORDER: [&str; 11] = [
    "C", "1B", "2B", "3B", "SS", "LF", "CF", "RF", "OF", "DH", "TWP",
];

fn draw_roster(f: &mut Frame, app: &App, page: &TeamPage, area: Rect) {
    let mut hitters: Vec<&RosterPlayer> = page.roster.iter().filter(|p| p.is_hitter()).collect();
    hitters.sort_by_key(|p| {
        (
            POS_ORDER
                .iter()
                .position(|x| *x == p.position)
                .unwrap_or(99),
            -(crate::stats::num(&p.hitting, "plateAppearances").unwrap_or(0.0) as i64),
        )
    });
    let mut pitchers: Vec<&RosterPlayer> = page.roster.iter().filter(|p| p.is_pitcher()).collect();
    pitchers.sort_by_key(|p| {
        let gs = crate::stats::num(&p.pitching, "gamesStarted").unwrap_or(0.0);
        let outs = crate::stats::num(&p.pitching, "outs").unwrap_or(0.0);
        (gs < 3.0, -(outs as i64))
    });
    let g = |s: &crate::stats::Stat, k: &str| text(s, k).unwrap_or_else(|| "—".into());

    let hcols: [(&str, usize); 11] = [
        ("G", 4),
        ("PA", 5),
        ("AVG", 6),
        ("OBP", 6),
        ("SLG", 6),
        ("OPS", 6),
        ("HR", 4),
        ("RBI", 5),
        ("SB", 4),
        ("BB", 4),
        ("SO", 5),
    ];
    let mut hl = vec![Line::from(
        std::iter::once(Span::styled(pad(" Pos  #   Hitter", 32), theme::meta()))
            .chain(
                hcols
                    .iter()
                    .map(|(c, w)| Span::styled(rpad(c, *w), theme::meta())),
            )
            .collect::<Vec<_>>(),
    )];
    for p in &hitters {
        let s = &p.hitting;
        let vals = [
            g(s, "gamesPlayed"),
            g(s, "plateAppearances"),
            g(s, "avg"),
            g(s, "obp"),
            g(s, "slg"),
            g(s, "ops"),
            g(s, "homeRuns"),
            g(s, "rbi"),
            g(s, "stolenBases"),
            g(s, "baseOnBalls"),
            g(s, "strikeOuts"),
        ];
        let mut spans = vec![
            Span::styled(pad(&format!(" {}", p.position), 5), theme::accent2()),
            Span::styled(pad(&p.number, 4), theme::meta()),
            Span::styled(pad(&p.name, 19), theme::text()),
            Span::styled(pad(&format!("{}/{}", p.bats, p.throws), 4), theme::meta()),
        ];
        for (v, (_, w)) in vals.iter().zip(hcols) {
            spans.push(Span::styled(rpad(v, w), theme::text()));
        }
        hl.push(Line::from(spans));
    }

    let pcols: [(&str, usize); 10] = [
        ("G", 4),
        ("GS", 4),
        ("W-L", 6),
        ("SV", 4),
        ("IP", 7),
        ("ERA", 6),
        ("WHIP", 6),
        ("SO", 5),
        ("BB", 4),
        ("HR", 4),
    ];
    let mut pl = vec![Line::from(
        std::iter::once(Span::styled(pad(" #   Pitcher", 28), theme::meta()))
            .chain(
                pcols
                    .iter()
                    .map(|(c, w)| Span::styled(rpad(c, *w), theme::meta())),
            )
            .collect::<Vec<_>>(),
    )];
    for p in &pitchers {
        let s = &p.pitching;
        let wl = format!("{}-{}", g(s, "wins"), g(s, "losses"));
        let vals = [
            g(s, "gamesPlayed"),
            g(s, "gamesStarted"),
            wl,
            g(s, "saves"),
            g(s, "inningsPitched"),
            g(s, "era"),
            g(s, "whip"),
            g(s, "strikeOuts"),
            g(s, "baseOnBalls"),
            g(s, "homeRuns"),
        ];
        let mut spans = vec![
            Span::styled(pad(&format!(" {}", p.number), 5), theme::meta()),
            Span::styled(pad(&p.name, 19), theme::text()),
            Span::styled(pad(&format!("{}HP", p.throws), 4), theme::meta()),
        ];
        for (v, (_, w)) in vals.iter().zip(pcols) {
            spans.push(Span::styled(rpad(v, w), theme::text()));
        }
        pl.push(Line::from(spans));
    }

    let title_h = format!("Hitters · {} active", hitters.len());
    let title_p = format!("Pitchers · {} active", pitchers.len());
    let s = app.team_scroll;
    if area.width >= 150 {
        let [a, b] = Layout::horizontal([Constraint::Length(90), Constraint::Min(60)]).areas(area);
        f.render_widget(
            Paragraph::new(hl)
                .block(panel(&title_h, true))
                .scroll((s, 0)),
            a,
        );
        f.render_widget(
            Paragraph::new(pl)
                .block(panel(&title_p, false))
                .scroll((s, 0)),
            b,
        );
    } else {
        let mut all = vec![Line::styled(format!(" {title_h}"), theme::accent_bold())];
        all.extend(hl);
        all.push(Line::raw(""));
        all.push(Line::styled(format!(" {title_p}"), theme::accent_bold()));
        all.extend(pl);
        let max = (all.len() as u16).saturating_sub(area.height.saturating_sub(2));
        f.render_widget(
            Paragraph::new(all)
                .block(panel("Roster · regular-season stats", true))
                .scroll((s.min(max), 0)),
            area,
        );
    }
}

fn draw_picker(f: &mut Frame, app: &App, sel: usize, area: Rect) {
    let Some(teams) = app.teams.as_ref() else {
        return;
    };
    let h = (teams.len() as u16 + 2).min(area.height.saturating_sub(2));
    let r = centered(area, 46, h);
    f.render_widget(Clear, r);
    let inner_h = h.saturating_sub(2) as usize;
    let first = sel.saturating_sub(inner_h.saturating_sub(1));
    let mut lines = Vec::new();
    for (n, (i, t)) in teams
        .iter()
        .enumerate()
        .skip(first)
        .take(inner_h)
        .enumerate()
    {
        app.hit(
            Rect::new(r.x + 1, r.y + 1 + n as u16, r.width.saturating_sub(2), 1),
            Click::PickerRow(i),
        );
        let fav = app.cfg.favorites.iter().any(|f| f == &t.abbreviation);
        let div = t
            .division
            .as_ref()
            .map(|d| division_name(d.id))
            .unwrap_or("");
        let mut line = Line::from(vec![
            Span::styled(if i == sel { " ▶ " } else { "   " }, theme::accent()),
            Span::styled(
                pad(&t.abbreviation, 5),
                if fav {
                    theme::accent_bold()
                } else {
                    theme::accent2()
                },
            ),
            Span::styled(
                pad(&t.name, 24),
                if fav {
                    theme::accent_bold()
                } else {
                    theme::text()
                },
            ),
            Span::styled(pad(div, 11), theme::meta()),
        ]);
        if i == sel {
            line = line.style(theme::selected());
        }
        lines.push(line);
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(panel("Team  ·  ⏎ open  esc close", true))
            .style(Style::new().bg(theme::BG0_H)),
        r,
    );
}
