// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Drawing. Each screen has its own module; this one holds the frame
//! (tab bar, status bar, help) and small text helpers.

mod boxscore;
mod gamecast;
mod gameview;
mod matchup;
mod pitches;
mod plays;
mod scoreboard;
mod scorecard;
mod spray;
mod standings;
mod winprob;

use crate::app::{App, Screen};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(theme::BG0)), area);
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_tabs(f, app, top);
    match app.screen {
        Screen::Scoreboard => scoreboard::draw(f, app, body),
        Screen::Standings => standings::draw(f, app, body),
        Screen::Game => gameview::draw(f, app, body),
    }
    draw_status(f, app, bottom);
    if app.help {
        draw_help(f, app, area);
    }
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled(" ⚾ Ballatui ", theme::accent_bold())];
    let mut tab = |n: &str, label: String, active: bool| {
        spans.push(Span::styled(" │ ", Style::new().fg(theme::BG3)));
        spans.push(Span::styled(n.to_string(), theme::key()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            label,
            if active {
                theme::accent_bold()
            } else {
                theme::secondary()
            },
        ));
    };
    tab("1", "Scores".into(), app.screen == Screen::Scoreboard);
    tab("2", "Standings".into(), app.screen == Screen::Standings);
    if let Some(g) = &app.game {
        let label = g
            .feed
            .as_ref()
            .map(|f| {
                format!(
                    "{} @ {}",
                    f.game_data.teams.away.abbreviation, f.game_data.teams.home.abbreviation
                )
            })
            .unwrap_or_else(|| "Game".into());
        tab("3", label, app.screen == Screen::Game);
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(theme::BG0_H)),
        area,
    );
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let hints: &[(&str, &str)] = match app.screen {
        Screen::Scoreboard => &[
            ("j/k", "game"),
            ("h/l", "day"),
            ("[ ]", "game day"),
            ("t", "today"),
            ("⏎", "open"),
            ("r", "refresh"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Screen::Standings => &[
            ("w", "division/wild card"),
            ("j/k", "scroll"),
            ("r", "refresh"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Screen::Game => &[
            ("c m i y w b s p", "tabs"),
            ("h/l", "pitch"),
            ("[ ]", "at-bat"),
            ("␣", "replay"),
            ("f", "live/end"),
            ("t", "team"),
            ("?", "help"),
            ("esc", "back"),
        ],
    };
    let mut spans = vec![Span::raw(" ")];
    if let Some((msg, _)) = &app.status {
        spans.push(Span::styled(msg.clone(), theme::accent()));
        spans.push(Span::raw("   "));
    }
    for (k, label) in hints {
        spans.push(Span::styled(*k, theme::key()));
        spans.push(Span::styled(format!(" {label}  "), theme::meta()));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(theme::BG0_H)),
        area,
    );
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Everywhere",
            &[
                ("1 2 3", "scores · standings · open game"),
                ("?", "this help (any key closes)"),
                ("q / Ctrl-c", "quit (in a game: back)"),
            ],
        ),
        (
            "Scores",
            &[
                ("j k  gg G", "pick a game"),
                ("h l  H L", "previous / next day · week"),
                ("[ ]", "previous / next day with games"),
                ("t", "today"),
                ("⏎ / o", "open the game"),
                ("r", "refresh"),
            ],
        ),
        (
            "Game",
            &[
                (
                    "c m y b s p",
                    "gamecast · matchup · spray · box · card · plays",
                ),
                ("Tab", "next tab"),
                ("h l", "step back / forward one pitch"),
                ("[ ]", "previous / next at-bat"),
                ("0", "first pitch"),
                ("f / $", "back to live (or the end)"),
                ("Space", "replay / pause (from the end, starts at pitch 1)"),
                ("R", "replay from the first pitch"),
                ("+ -", "replay speed"),
                ("t", "team: box, scorecard · spray: both/away/home"),
                ("a", "spray: batter now up · pitches: follow the pitcher"),
                ("j k", "pitches: pick a pitcher"),
                ("j k  Ctrl-d/u", "scroll · in Plays, ⏎ jumps to that play"),
                ("< >", "scroll the scorecard sideways"),
                ("r  esc", "refresh · back"),
            ],
        ),
        (
            "Standings",
            &[("w / Tab", "division ↔ wild card"), ("j k", "scroll")],
        ),
    ];
    let mut lines = Vec::new();
    for (title, keys) in sections {
        lines.push(Line::styled(*title, theme::accent_bold()));
        for (k, d) in *keys {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", pad(k, 15)), theme::key()),
                Span::styled(*d, theme::text()),
            ]));
        }
        lines.push(Line::raw(""));
    }
    if let Some(p) = crate::config::config_path() {
        lines.push(Line::styled(
            format!("Settings: {}", p.display()),
            theme::meta(),
        ));
    }
    let _ = app;
    let h = (lines.len() as u16 + 2).min(area.height);
    let w = 68.min(area.width);
    let r = centered(area, w, h);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines)
            .block(panel("Keys", true).padding(ratatui::widgets::Padding::horizontal(1)))
            .style(Style::new().bg(theme::BG0_H)),
        r,
    );
}

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

pub fn panel(title: &str, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(Span::styled(
            format!(" {title} "),
            if focused {
                theme::accent_bold()
            } else {
                theme::secondary()
            },
        ))
}

/// Left-aligns `s` in `w` columns, cutting it with an ellipsis if needed.
pub fn pad(s: &str, w: usize) -> String {
    let width = s.width();
    if width <= w {
        format!("{s}{}", " ".repeat(w - width))
    } else {
        let mut out = String::new();
        let mut used = 0;
        for c in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + cw + 1 > w {
                break;
            }
            out.push(c);
            used += cw;
        }
        out.push('…');
        used += 1;
        out.push_str(&" ".repeat(w.saturating_sub(used)));
        out
    }
}

/// Right-aligns `s` in `w` columns.
pub fn rpad(s: &str, w: usize) -> String {
    let width = s.width();
    if width >= w {
        s.to_string()
    } else {
        format!("{}{s}", " ".repeat(w - width))
    }
}

/// Centers `s` in `w` columns.
pub fn cpad(s: &str, w: usize) -> String {
    let width = s.width();
    if width >= w {
        return s.to_string();
    }
    let left = (w - width) / 2;
    format!("{}{s}{}", " ".repeat(left), " ".repeat(w - width - left))
}

pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

pub fn opt(n: Option<u32>) -> String {
    n.map(|v| v.to_string()).unwrap_or_default()
}

/// Local start time of a game from its UTC timestamp.
pub fn local_time(utc: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(utc)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%-I:%M %p")
                .to_string()
        })
        .unwrap_or_default()
}

pub fn message(f: &mut Frame, area: Rect, text: &str, style: Style) {
    let r = centered(area, (text.width() as u16 + 4).min(area.width), 1);
    f.render_widget(Paragraph::new(text.to_string()).style(style), r);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding() {
        assert_eq!(pad("abc", 5), "abc  ");
        assert_eq!(pad("abcdef", 4), "abc…");
        assert_eq!(rpad("7", 3), "  7");
        assert_eq!(cpad("ab", 6), "  ab  ");
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(23), "23rd");
    }
}
