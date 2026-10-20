// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

use super::{gameview, local_time, message, ordinal, pad, panel, rpad};
use crate::app::App;
use crate::model::{ScheduleGame, ScheduleSide};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let wide = area.width >= 120;
    let [list_area, detail_area] = if wide {
        Layout::horizontal([Constraint::Min(70), Constraint::Length(54)]).areas(area)
    } else {
        [area, Rect::default()]
    };
    let today = crate::app::baseball_today(app.cfg.day_rollover_hour);
    let mut title = app.date.format("%a %b %-d, %Y").to_string();
    if app.date == today {
        title.push_str(" · today");
    }
    let block = panel(&format!("◀ {title} ▶"), true);
    let inner = block.inner(list_area);
    f.render_widget(block, list_area);

    let Some(games) = app.games.as_ref() else {
        let (text, style) = match &app.sched_error {
            Some(e) => (format!("Couldn't load scores: {e}"), theme::error()),
            None => ("Loading scores…".to_string(), theme::meta()),
        };
        message(f, inner, &text, style);
        return;
    };
    if games.is_empty() {
        message(
            f,
            inner,
            "No games on this day  ·  ] next game day  ·  [ previous",
            theme::meta(),
        );
        return;
    }

    // Each game takes three rows: two teams and a gap.
    let per = 3usize;
    let visible = (inner.height as usize / per).max(1);
    let first = app.sel.saturating_sub(visible - 1);
    let mut lines = Vec::new();
    for (i, g) in games.iter().enumerate().skip(first).take(visible) {
        let sel = i == app.sel;
        let mut rows = game_rows(app, g, inner.width as usize);
        if sel {
            for r in rows.iter_mut() {
                *r = std::mem::take(r).style(theme::selected());
            }
        }
        lines.extend(rows);
        lines.push(Line::raw(""));
    }
    f.render_widget(Paragraph::new(lines), inner);

    if wide && let Some(g) = app.selected_game() {
        draw_detail(f, app, g, detail_area);
    }
}

fn is_fav(app: &App, side: &ScheduleSide) -> bool {
    app.cfg
        .favorites
        .iter()
        .any(|f| f == &side.team.abbreviation)
}

fn game_rows(app: &App, g: &ScheduleGame, width: usize) -> Vec<Line<'static>> {
    let st = &g.status;
    let ls = g.linescore.as_ref();
    let started = st.is_live() || st.is_final();
    let mut out = Vec::new();
    for home in [false, true] {
        let side = g.teams.get(home);
        let fav = is_fav(app, side);
        let winner = side.is_winner == Some(true);
        let loser = st.is_final() && side.is_winner == Some(false);
        let name_style = if fav {
            theme::accent_bold()
        } else if loser {
            theme::secondary()
        } else {
            theme::title()
        };
        let mark = if fav { "★ " } else { "  " };
        let record = side
            .league_record
            .as_ref()
            .map(|r| format!("{}-{}", r.wins, r.losses))
            .unwrap_or_default();
        let score = if started {
            side.score
                .map(|s| s.to_string())
                .unwrap_or_else(|| "0".into())
        } else {
            String::new()
        };
        let score_style = if winner || st.is_live() {
            theme::title()
        } else {
            theme::secondary()
        };
        let mut spans = vec![
            Span::styled(format!(" {mark}"), theme::accent()),
            Span::styled(pad(&side.team.abbreviation, 5), name_style),
            Span::styled(pad(&record, 8), theme::meta()),
            Span::styled(rpad(&score, 3), score_style.add_modifier(Modifier::BOLD)),
            Span::raw("   "),
        ];
        // Status column.
        let (status, status_style) = if !home {
            status_text(g)
        } else {
            status_second_line(g)
        };
        spans.push(Span::styled(pad(&status, 15), status_style));
        // Info column.
        let info_w = width.saturating_sub(2 + 5 + 8 + 3 + 3 + 15 + 1);
        let info = if !home {
            info_first(g)
        } else {
            info_second(g, ls)
        };
        spans.push(Span::styled(pad(&info, info_w), theme::meta()));
        out.push(Line::from(spans));
    }
    out
}

fn status_text(g: &ScheduleGame) -> (String, Style) {
    let st = &g.status;
    if st.is_final() {
        let inn = g
            .linescore
            .as_ref()
            .and_then(|l| l.current_inning)
            .unwrap_or(9);
        let sched = g.scheduled_innings.unwrap_or(9);
        let s = if inn != sched {
            format!("Final/{inn}")
        } else {
            "Final".into()
        };
        if st.detailed_state != "Final" && st.detailed_state != "Game Over" {
            return (st.detailed_state.clone(), theme::warn());
        }
        (s, theme::secondary())
    } else if st.is_live() {
        match g.linescore.as_ref() {
            Some(l) if st.detailed_state == "In Progress" => {
                let half = l.inning_state.as_deref().unwrap_or("");
                let arrow = match half {
                    "Top" => "▲",
                    "Bottom" => "▼",
                    "Middle" => "Mid",
                    "End" => "End",
                    _ => "",
                };
                let inn = l.current_inning.map(ordinal).unwrap_or_default();
                (format!("{arrow} {inn}"), theme::accent_bold())
            }
            _ => (st.detailed_state.clone(), theme::accent()),
        }
    } else if matches!(
        st.detailed_state.as_str(),
        "Postponed" | "Suspended" | "Cancelled"
    ) || st.detailed_state.starts_with("Delayed")
    {
        (st.detailed_state.clone(), theme::warn())
    } else {
        (local_time(&g.game_date), theme::text())
    }
}

fn status_second_line(g: &ScheduleGame) -> (String, Style) {
    let st = &g.status;
    if st.is_live()
        && st.detailed_state == "In Progress"
        && let Some(l) = &g.linescore
    {
        let outs = l.outs.unwrap_or(0);
        let mut s = format!("{outs} out{}", if outs == 1 { "" } else { "s" });
        if let Some(o) = &l.offense {
            let b = [&o.first, &o.second, &o.third];
            let on: String = b
                .iter()
                .rev()
                .map(|x| if x.is_some() { '◆' } else { '◇' })
                .collect();
            s = format!("{s} {on}");
        }
        return (s, theme::text());
    }
    (String::new(), theme::meta())
}

fn info_first(g: &ScheduleGame) -> String {
    let mut parts = Vec::new();
    if g.game_type != "R" {
        if let (Some(desc), Some(n)) = (&g.series_description, g.series_game_number) {
            parts.push(format!("{desc} · Game {n}"));
        }
    } else if g.double_header != "N" {
        parts.push(format!("Game {}", g.game_number));
    }
    if parts.is_empty() {
        parts.push(g.venue.name.clone().unwrap_or_default());
    }
    parts.join(" · ")
}

fn info_second(g: &ScheduleGame, ls: Option<&crate::model::Linescore>) -> String {
    let st = &g.status;
    if st.is_final() {
        if let Some(d) = &g.decisions {
            let mut s = Vec::new();
            if let Some(w) = &d.winner {
                s.push(format!("W {}", last(&w.full_name)));
            }
            if let Some(l) = &d.loser {
                s.push(format!("L {}", last(&l.full_name)));
            }
            if let Some(sv) = &d.save {
                s.push(format!("S {}", last(&sv.full_name)));
            }
            let mut line = s.join("  ");
            if g.game_type != "R"
                && let Some(ss) = g.series_status.as_ref().and_then(|s| s.result.clone())
            {
                line = format!("{line}  ·  {ss}");
            }
            return line;
        }
    } else if st.is_live() {
        if let Some(l) = ls {
            let p = l.defense.as_ref().and_then(|d| d.pitcher.as_ref());
            let b = l.offense.as_ref().and_then(|o| o.batter.as_ref());
            if let (Some(p), Some(b)) = (p, b) {
                return format!("P {}  ·  AB {}", last(&p.full_name), last(&b.full_name));
            }
        }
    } else {
        let a = g
            .teams
            .away
            .probable_pitcher
            .as_ref()
            .map(|p| last(&p.full_name));
        let h = g
            .teams
            .home
            .probable_pitcher
            .as_ref()
            .map(|p| last(&p.full_name));
        if a.is_some() || h.is_some() {
            return format!(
                "{} vs {}",
                a.unwrap_or_else(|| "TBD".into()),
                h.unwrap_or_else(|| "TBD".into())
            );
        }
    }
    String::new()
}

fn last(full: &str) -> String {
    match full.split_once(' ') {
        Some((_, rest)) if !rest.is_empty() => rest.to_string(),
        _ => full.to_string(),
    }
}

fn draw_detail(f: &mut Frame, app: &App, g: &ScheduleGame, area: Rect) {
    let title = format!(
        "{} @ {}",
        g.teams.away.team.team_name, g.teams.home.team.team_name
    );
    let block = panel(&title, false).padding(ratatui::widgets::Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines: Vec<Line> = Vec::new();
    let away = &g.teams.away.team.abbreviation;
    let home = &g.teams.home.team.abbreviation;
    if let Some(ls) = &g.linescore
        && !ls.innings.is_empty()
    {
        let line: Vec<[Option<u32>; 2]> = ls
            .innings
            .iter()
            .map(|i| [i.away.runs, i.home.runs])
            .collect();
        let rhe = [
            [ls.teams.away.runs, ls.teams.away.hits, ls.teams.away.errors],
            [ls.teams.home.runs, ls.teams.home.hits, ls.teams.home.errors],
        ];
        lines.extend(gameview::linescore_lines(
            [away, home],
            &line,
            rhe,
            ls.scheduled_innings.unwrap_or(9),
            inner.width as usize,
            [is_fav(app, &g.teams.away), is_fav(app, &g.teams.home)],
            g.status.is_final(),
        ));
        lines.push(Line::raw(""));
    }
    let kv = |k: &str, v: String| {
        Line::from(vec![
            Span::styled(pad(k, 10), theme::meta()),
            Span::styled(v, theme::text()),
        ])
    };
    lines.push(kv("Status", g.status.detailed_state.clone()));
    lines.push(kv("Start", local_time(&g.game_date)));
    if let Some(v) = &g.venue.name {
        lines.push(kv("Venue", v.clone()));
    }
    if let Some(d) = &g.series_description
        && g.game_type != "R"
    {
        lines.push(kv("Series", d.clone()));
    }
    if let Some(s) = g.series_status.as_ref().and_then(|s| s.result.clone())
        && g.game_type != "R"
    {
        lines.push(kv("", s));
    }
    if g.status.is_preview() {
        for home in [false, true] {
            let side = g.teams.get(home);
            if let Some(p) = &side.probable_pitcher {
                lines.push(kv(
                    if home { "Home SP" } else { "Away SP" },
                    p.full_name.clone(),
                ));
            }
        }
    }
    if let Some(d) = &g.decisions {
        if let Some(w) = &d.winner {
            lines.push(kv("Win", w.full_name.clone()));
        }
        if let Some(l) = &d.loser {
            lines.push(kv("Loss", l.full_name.clone()));
        }
        if let Some(s) = &d.save {
            lines.push(kv("Save", s.full_name.clone()));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("⏎ ", theme::key()),
        Span::styled("open gamecast", theme::meta()),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}
