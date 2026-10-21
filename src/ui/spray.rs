// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Spray chart: every ball in play up to the shown moment on a drawing of
//! the park (its real fence distances), with a list of the balls beside it.

use super::gameview::Ctx;
use super::{pad, panel, rpad};
use crate::app::{GameView, SprayTeam};
use crate::game::{BattedBall, is_barrel, last_name};
use crate::model::FieldInfo;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::widgets::canvas::{Canvas, Line as CLine, Points};

/// Exit velocity counted as hard-hit (Statcast).
const HARD_HIT: f64 = 95.0;

pub fn draw(f: &mut Frame, ctx: &Ctx, g: &GameView, area: Rect) {
    let balls = g.spray_balls();
    let sel = if balls.is_empty() {
        None
    } else {
        Some(g.spray_sel.min(balls.len() - 1))
    };
    let field = ctx
        .stats
        .venues
        .get(&ctx.feed.game_data.venue.id)
        .and_then(|v| v.ready().copied().flatten())
        .unwrap_or_default();

    // The field wants roughly square physical proportions; terminal cells
    // are about twice as tall as wide.
    let chart_w = (area.height.saturating_sub(2) * 2 + 2)
        .min(area.width * 6 / 10)
        .max(30);
    let [chart, list] =
        Layout::horizontal([Constraint::Length(chart_w), Constraint::Min(30)]).areas(area);
    draw_chart(f, ctx, &balls, sel, &field, chart);
    draw_list(f, ctx, g, &balls, sel, list);
}

fn ball_style(b: &BattedBall) -> (&'static str, Color) {
    match b.event_type.as_str() {
        "single" => ("●", theme::GREEN),
        "double" => ("●", theme::AQUA),
        "triple" => ("●", theme::BLUE),
        "home_run" => ("◆", theme::ORANGE),
        "field_error" | "fielders_choice" | "force_out" | "fielders_choice_out" => {
            ("○", theme::BLUE)
        }
        "" => ("●", theme::FG3),
        _ => ("○", theme::FG4),
    }
}

fn draw_chart(
    f: &mut Frame,
    ctx: &Ctx,
    balls: &[BattedBall],
    sel: Option<usize>,
    field: &FieldInfo,
    area: Rect,
) {
    let venue = ctx.feed.game_data.venue.name.clone().unwrap_or_default();
    let block = panel(&format!("Spray · {venue}"), false)
        .title_bottom(Line::styled(" fielded spot for grounders ", theme::meta()));
    let inner = block.inner(area);
    // Bounds that keep the field's proportions in this many cells.
    let deepest = (-45..=45)
        .map(|a| field.fence_at(a as f64))
        .fold(0.0, f64::max);
    let y_lo = -25.0;
    let mut y_hi = deepest + 20.0;
    let ratio = inner.width.max(1) as f64 / (2.0 * inner.height.max(1) as f64);
    let mut half_x = (y_hi - y_lo) * ratio / 2.0;
    let need_x = 250.0;
    if half_x < need_x {
        half_x = need_x;
        y_hi = y_lo + 2.0 * half_x / ratio;
    }
    let fence: Vec<(f64, f64)> = (0..=60)
        .map(|i| {
            let a = -45.0 + i as f64 * 1.5;
            let d = field.fence_at(a);
            let r = a.to_radians();
            (d * r.sin(), d * r.cos())
        })
        .collect();
    let field = *field;
    let balls = balls.to_vec();
    let feed = ctx.feed;
    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .background_color(theme::BG0)
        .x_bounds([-half_x, half_x])
        .y_bounds([y_lo, y_hi])
        .paint(move |c| {
            // Foul lines, fence, infield.
            let (lx, ly) = fence[0];
            let (rx, ry) = fence[fence.len() - 1];
            c.draw(&CLine::new(0.0, 0.0, lx, ly, theme::BG4));
            c.draw(&CLine::new(0.0, 0.0, rx, ry, theme::BG4));
            for w in fence.windows(2) {
                c.draw(&CLine::new(w[0].0, w[0].1, w[1].0, w[1].1, theme::FG4));
            }
            let b = 90.0 / 2f64.sqrt();
            let diamond = [(0.0, 0.0), (b, b), (0.0, 2.0 * b), (-b, b), (0.0, 0.0)];
            for w in diamond.windows(2) {
                c.draw(&CLine::new(w[0].0, w[0].1, w[1].0, w[1].1, theme::BG3));
            }
            let arc: Vec<(f64, f64)> = (-36..=36)
                .map(|i| {
                    let t = (i as f64 * 2.0).to_radians();
                    (95.0 * t.sin(), 60.5 + 95.0 * t.cos())
                })
                .collect();
            c.draw(&Points {
                coords: &arc,
                color: theme::BG2,
            });
            c.layer();
            // Fence distances.
            for (a, d) in [
                (-45.0, field.fence_at(-45.0)),
                (0.0, field.fence_at(0.0)),
                (45.0, field.fence_at(45.0)),
            ] {
                let r = f64::to_radians(a);
                let (x, y) = ((d + 12.0) * r.sin(), (d + 12.0) * r.cos());
                let label = format!("{d:.0}");
                let shift = match a {
                    a if a < 0.0 => 0.0,
                    0.0 => -(label.len() as f64) * half_x / 60.0,
                    _ => -(label.len() as f64) * half_x / 30.0,
                };
                c.print(x + shift, y, Span::styled(label, theme::meta()));
            }
            // Balls, the selected one last so it stays on top.
            let mut order: Vec<usize> = (0..balls.len()).collect();
            if let Some(s) = sel {
                order.retain(|i| *i != s);
                order.push(s);
            }
            for i in order {
                let bb = &balls[i];
                let (glyph, color) = ball_style(bb);
                let mut style = Style::new().fg(color);
                if Some(i) == sel {
                    style = style.bg(theme::BG2).add_modifier(Modifier::BOLD);
                }
                let x = bb.x.clamp(-half_x * 0.98, half_x * 0.98);
                let y = bb.y.clamp(y_lo, y_hi * 0.98);
                c.print(x, y, Span::styled(glyph, style));
                if Some(i) == sel {
                    let name = last_name(feed, bb.batter, &bb.batter_name);
                    c.print(
                        x + half_x / 25.0,
                        y,
                        Span::styled(format!(" {name}"), theme::title().bg(theme::BG1)),
                    );
                }
            }
        });
    f.render_widget(canvas, area);
}

fn draw_list(
    f: &mut Frame,
    ctx: &Ctx,
    g: &GameView,
    balls: &[BattedBall],
    sel: Option<usize>,
    area: Rect,
) {
    let block = panel("Balls in play  ·  ⏎ show in gamecast", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let gd = &ctx.feed.game_data;
    let mut head: Vec<Line> = Vec::new();

    // Summary.
    let evs: Vec<f64> = balls.iter().filter_map(|b| b.launch_speed).collect();
    let avg_ev = if evs.is_empty() {
        None
    } else {
        Some(evs.iter().sum::<f64>() / evs.len() as f64)
    };
    let hard = evs.iter().filter(|e| **e >= HARD_HIT).count();
    let barrels = balls
        .iter()
        .filter(
            |b| matches!((b.launch_speed, b.launch_angle), (Some(e), Some(a)) if is_barrel(e, a)),
        )
        .count();
    let max_dist = balls.iter().filter_map(|b| b.distance).fold(0.0, f64::max);
    let mut sum = vec![Span::styled(
        format!(" {} balls in play", balls.len()),
        theme::title(),
    )];
    if let Some(a) = avg_ev {
        sum.push(Span::styled(format!(" · avg EV {a:.1}"), theme::text()));
        sum.push(Span::styled(
            format!(
                " · hard-hit {hard} ({:.0}%)",
                hard as f64 * 100.0 / evs.len() as f64
            ),
            theme::text(),
        ));
    }
    sum.push(Span::styled(format!(" · barrels {barrels}"), theme::text()));
    if max_dist > 0.0 {
        sum.push(Span::styled(
            format!(" · longest {max_dist:.0} ft"),
            theme::text(),
        ));
    }
    head.push(Line::from(sum));

    // Filters.
    let team = match g.spray_team {
        SprayTeam::Both => "both teams".to_string(),
        SprayTeam::Away => gd.teams.away.team_name.clone(),
        SprayTeam::Home => gd.teams.home.team_name.clone(),
    };
    let who = if g.spray_batter_only {
        ctx.feed
            .live_data
            .plays
            .all_plays
            .get(ctx.cursor.play)
            .map(|p| last_name(ctx.feed, p.matchup.batter.id, &p.matchup.batter.full_name))
            .unwrap_or_else(|| "this batter".into())
    } else {
        "every batter".into()
    };
    head.push(Line::from(vec![
        Span::styled(" t ", theme::key()),
        Span::styled(pad(&team, 14), theme::accent2()),
        Span::styled(" a ", theme::key()),
        Span::styled(who, theme::accent2()),
    ]));
    head.push(Line::raw(""));
    head.push(Line::from(vec![Span::styled(
        format!(
            "   {}{}{}{}{}{}{}",
            pad("Inn", 5),
            pad("Batter", 14),
            pad("Result", 9),
            rpad("EV", 6),
            rpad("LA", 5),
            rpad("Dist", 6),
            "  Type"
        ),
        theme::meta(),
    )]));

    // Rows, keeping the selection in view.
    let legend_h = 3u16;
    let rows_h = inner.height.saturating_sub(head.len() as u16 + legend_h) as usize;
    let first = sel
        .map(|s| s.saturating_sub(rows_h.saturating_sub(1)))
        .unwrap_or(0);
    let mut lines = head;
    for (i, b) in balls.iter().enumerate().skip(first).take(rows_h) {
        let selected = Some(i) == sel;
        let (glyph, color) = ball_style(b);
        let inn = format!("{}{}", if b.top { "▲" } else { "▼" }, b.inning);
        let name = last_name(ctx.feed, b.batter, &b.batter_name);
        let ev = b
            .launch_speed
            .map(|v| format!("{v:.1}"))
            .unwrap_or_default();
        let la = b
            .launch_angle
            .map(|v| format!("{v:.0}°"))
            .unwrap_or_default();
        let dist = b.distance.map(|v| format!("{v:.0}")).unwrap_or_default();
        let barrel =
            matches!((b.launch_speed, b.launch_angle), (Some(e), Some(a)) if is_barrel(e, a));
        let ev_style = if b.launch_speed.is_some_and(|e| e >= HARD_HIT) {
            theme::title()
        } else {
            theme::secondary()
        };
        let kind = b
            .trajectory
            .as_deref()
            .unwrap_or("")
            .replace("_ball", "")
            .replace('_', " ");
        let mut line = Line::from(vec![
            Span::styled(if selected { " ▶" } else { "  " }, theme::accent()),
            Span::styled(glyph.to_string(), Style::new().fg(color)),
            Span::styled(pad(&inn.to_string(), 5), theme::accent2()),
            Span::styled(pad(&name, 14), theme::text()),
            Span::styled(pad(&b.result, 9), Style::new().fg(color)),
            Span::styled(rpad(&ev, 6), ev_style),
            Span::styled(rpad(&la, 5), theme::secondary()),
            Span::styled(rpad(&dist, 6), theme::secondary()),
            Span::styled(format!("  {kind}"), theme::meta()),
            Span::styled(
                if barrel { "  barrel" } else { "" },
                Style::new().fg(theme::ORANGE),
            ),
        ]);
        if selected {
            line = line.style(theme::selected());
        }
        lines.push(line);
    }
    if balls.is_empty() {
        lines.push(Line::styled("   No balls in play yet.", theme::meta()));
    }
    f.render_widget(Paragraph::new(lines), inner);

    // Legend pinned to the bottom.
    let legend = vec![
        Line::from(vec![
            Span::styled(" ● ", Style::new().fg(theme::GREEN)),
            Span::styled("1B ", theme::meta()),
            Span::styled("● ", Style::new().fg(theme::AQUA)),
            Span::styled("2B ", theme::meta()),
            Span::styled("● ", Style::new().fg(theme::BLUE)),
            Span::styled("3B ", theme::meta()),
            Span::styled("◆ ", Style::new().fg(theme::ORANGE)),
            Span::styled("HR ", theme::meta()),
            Span::styled("○ ", Style::new().fg(theme::FG4)),
            Span::styled("out ", theme::meta()),
            Span::styled("○ ", Style::new().fg(theme::BLUE)),
            Span::styled("error / FC", theme::meta()),
        ]),
        Line::styled(
            " EV in bold = hard-hit (95+ mph) · barrel = Statcast barrel zone",
            theme::meta(),
        ),
    ];
    let la = Rect {
        y: inner.y + inner.height.saturating_sub(legend_h - 1),
        height: (legend_h - 1).min(inner.height),
        ..inner
    };
    f.render_widget(Paragraph::new(legend), la);
}
