// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Every play so far, grouped by half-inning. Enter jumps the gamecast to
//! the selected play.

use super::gameview::Ctx;
use super::{ordinal, pad, panel};
use crate::game::play_log;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

/// Draws the list; returns the number of selectable lines and the
/// selected play index.
pub fn draw(f: &mut Frame, ctx: &Ctx, sel: usize, area: Rect) -> (usize, Option<usize>) {
    let block = panel("Plays  ·  ⏎ show in gamecast", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let log = play_log(&ctx.feed.live_data.plays.all_plays, ctx.cursor);
    let gd = &ctx.feed.game_data;
    let mut lines: Vec<Line> = Vec::new();
    let mut sel_line = 0usize;
    let mut half = None;
    for (i, l) in log.iter().enumerate() {
        let key = (l.inning, l.top);
        if half != Some(key) {
            half = Some(key);
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            let team = if l.top {
                &gd.teams.away
            } else {
                &gd.teams.home
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        " {} {} {}",
                        if l.top { "▲" } else { "▼" },
                        if l.top { "Top" } else { "Bottom" },
                        ordinal(l.inning)
                    ),
                    theme::accent_bold(),
                ),
                Span::styled(format!("  {}", team.team_name), theme::meta()),
            ]));
        }
        if i == sel {
            sel_line = lines.len();
        }
        let outs = if l.is_action {
            String::new()
        } else {
            format!("{} out", l.outs)
        };
        let style = if l.scoring {
            theme::accent()
        } else if l.is_action {
            theme::meta()
        } else {
            theme::text()
        };
        let mut spans = vec![
            Span::styled(if i == sel { " ▶ " } else { "   " }, theme::accent()),
            Span::styled(pad(&outs, 7), theme::meta()),
            Span::styled(l.text.clone(), style),
        ];
        if let Some((a, h)) = l.score {
            spans.push(Span::styled(
                format!(
                    "  {} {a}, {} {h}",
                    gd.teams.away.abbreviation, gd.teams.home.abbreviation
                ),
                theme::accent_bold(),
            ));
        }
        let mut line = Line::from(spans);
        if i == sel {
            line = line.style(theme::selected().add_modifier(Modifier::BOLD));
        }
        lines.push(line);
    }
    if log.is_empty() {
        lines.push(Line::styled(" No plays yet.", theme::meta()));
    }
    // Keep the selection in view (lines can wrap, so leave some slack).
    let h = inner.height as usize;
    let top = sel_line.saturating_sub(h / 3);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((top as u16, 0)),
        inner,
    );
    (log.len(), log.get(sel).map(|l| l.play))
}
