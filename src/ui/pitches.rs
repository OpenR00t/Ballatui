// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! A pitcher's game, pitch type by pitch type: usage, velocity against his
//! season average, spin, movement, called-strike-plus-whiff and whiff
//! rates, velocity by inning, and a movement chart.

use super::gameview::Ctx;
use super::{pad, panel, rpad};
use crate::app::GameView;
use crate::game::{GamePitcher, PitchRec, is_csw, is_swing, is_whiff, last_name};
use crate::stats::Load;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::widgets::canvas::{Canvas, Line as CLine, Points};
use std::collections::BTreeMap;

/// A color per pitch type, close to the usual Statcast scheme within the
/// Gruvbox palette (yellow is kept for warnings).
pub fn type_color(code: &str) -> Color {
    match code {
        "FF" | "FA" => theme::RED,
        "SI" | "FT" => theme::ORANGE,
        "FC" => Color::Rgb(0xd6, 0x5d, 0x0e),
        "SL" => theme::PURPLE,
        "ST" | "SV" => Color::Rgb(0xb1, 0x62, 0x86),
        "CU" | "CS" => theme::BLUE,
        "KC" => Color::Rgb(0x45, 0x85, 0x88),
        "CH" => theme::GREEN,
        "FS" | "FO" => theme::AQUA,
        "KN" | "EP" | "SC" => theme::FG0,
        _ => theme::FG4,
    }
}

/// Short names so the table fits.
fn short_name(name: &str) -> String {
    match name {
        "Four-Seam Fastball" | "Four-seam FB" => "4-Seam".into(),
        "Two-Seam Fastball" => "2-Seam".into(),
        "Knuckle Curve" => "Knuckle Curve".into(),
        "Slow Curve" => "Slow Curve".into(),
        "Split-Finger" => "Splitter".into(),
        other => other.to_string(),
    }
}

/// Per-type totals.
#[derive(Default)]
struct TypeLine {
    name: String,
    n: u32,
    speed: Vec<f64>,
    spin: Vec<f64>,
    ivb: Vec<f64>,
    hb: Vec<f64>,
    csw: u32,
    swings: u32,
    whiffs: u32,
    evs: Vec<f64>,
}

fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}

fn pct(a: u32, b: u32) -> String {
    if b == 0 {
        "—".into()
    } else {
        format!("{:.0}%", a as f64 * 100.0 / b as f64)
    }
}

/// Width of the pitch table: name, then the stat columns.
const NAME_W: usize = 16;
const COLS: [(&str, usize); 11] = [
    ("#", 4),
    ("%", 5),
    ("Velo", 6),
    ("Max", 6),
    ("Δszn", 6),
    ("Spin", 6),
    ("IVB", 6),
    ("HB", 6),
    ("CSW", 5),
    ("Whf%", 6),
    ("EV", 6),
];
const TABLE_W: u16 = 16 + 4 + 5 + 6 + 6 + 6 + 6 + 6 + 6 + 5 + 6 + 6 + 1;

pub fn draw(f: &mut Frame, ctx: &Ctx, g: &GameView, area: Rect) {
    let (recs, mut pitchers) = g.pitch_data();
    if pitchers.is_empty() {
        super::message(f, area, "No pitches yet.", theme::meta());
        return;
    }
    pitchers.sort_by_key(|p| p.home);
    let shown = g.shown_pitcher().unwrap_or(pitchers[0].id);
    let current = g.current_pitcher();
    let mine: Vec<&PitchRec> = recs.iter().filter(|r| r.pitcher == shown).collect();

    // The table gets the room it needs; the chart takes what's left.
    let chart_w = area
        .width
        .saturating_sub(22 + TABLE_W + 2)
        .min(area.height.saturating_sub(2) * 2 + 2);
    let [list, table, chart] = Layout::horizontal([
        Constraint::Length(22),
        Constraint::Min(40),
        Constraint::Length(if chart_w >= 20 { chart_w } else { 0 }),
    ])
    .areas(area);
    draw_pitchers(
        f,
        ctx,
        &pitchers,
        shown,
        current,
        g.pitch_sel.is_none(),
        list,
    );
    draw_table(f, ctx, shown, &mine, table);
    draw_movement(f, &mine, chart);
}

fn draw_pitchers(
    f: &mut Frame,
    ctx: &Ctx,
    pitchers: &[GamePitcher],
    shown: u64,
    current: Option<u64>,
    following: bool,
    area: Rect,
) {
    let mut lines = Vec::new();
    let mut last_side = None;
    for p in pitchers {
        if last_side != Some(p.home) {
            last_side = Some(p.home);
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            let team = ctx.feed.game_data.teams.get(p.home);
            lines.push(Line::styled(
                format!(" {}", team.team_name),
                theme::accent2(),
            ));
        }
        let y = area.y + 1 + lines.len() as u16;
        if y + 1 < area.y + area.height {
            ctx.hit(
                Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1),
                crate::app::Click::Pitcher(p.id),
            );
        }
        let sel = p.id == shown;
        let mark = if Some(p.id) == current { "●" } else { " " };
        let name = last_name(ctx.feed, p.id, &p.name);
        let mut line = Line::from(vec![
            Span::styled(if sel { " ▶" } else { "  " }, theme::accent()),
            Span::styled(format!("{mark} "), theme::accent()),
            Span::styled(
                pad(&name, 12),
                if sel { theme::title() } else { theme::text() },
            ),
            Span::styled(rpad(&p.pitches.to_string(), 4), theme::secondary()),
        ]);
        if sel {
            line = line.style(theme::selected());
        }
        lines.push(line);
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" ● ", theme::accent()),
        Span::styled("pitching now", theme::meta()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" j/k ", theme::key()),
        Span::styled("pick  ", theme::meta()),
        Span::styled("a ", theme::key()),
        Span::styled(
            if following { "following" } else { "follow" },
            theme::meta(),
        ),
    ]));
    f.render_widget(Paragraph::new(lines).block(panel("Pitchers", false)), area);
}

fn draw_table(f: &mut Frame, ctx: &Ctx, pitcher: u64, mine: &[&PitchRec], area: Rect) {
    let name = last_name(ctx.feed, pitcher, "");
    let block = panel(&format!("Pitches · {name}"), true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut by_type: BTreeMap<String, TypeLine> = BTreeMap::new();
    for r in mine {
        let t = by_type.entry(r.code.clone()).or_default();
        t.name = r.name.clone();
        t.n += 1;
        t.speed.extend(r.speed);
        t.spin.extend(r.spin);
        t.ivb.extend(r.ivb);
        t.hb.extend(r.hb_arm);
        t.csw += is_csw(&r.call) as u32;
        t.swings += is_swing(&r.call) as u32;
        t.whiffs += is_whiff(&r.call) as u32;
        t.evs.extend(r.launch_speed);
    }
    let mut rows: Vec<(String, TypeLine)> = by_type.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.n));
    let total = mine.len() as u32;
    let season = match ctx.stats.arsenal.get(&(pitcher, ctx.season)) {
        Some(Load::Ready(a)) => a.clone(),
        _ => Vec::new(),
    };

    // Summary.
    let strikes = mine
        .iter()
        .filter(|r| !matches!(r.call.as_str(), "B" | "*B" | "V" | "P" | "I" | "H"))
        .count() as u32;
    let csw: u32 = rows.iter().map(|r| r.1.csw).sum();
    let whiffs: u32 = rows.iter().map(|r| r.1.whiffs).sum();
    let swings: u32 = rows.iter().map(|r| r.1.swings).sum();
    let hand = ctx
        .feed
        .game_data
        .players
        .get(&format!("ID{pitcher}"))
        .and_then(|p| p.pitch_hand.as_ref())
        .map(|h| format!("{}HP", h.code))
        .unwrap_or_default();
    let full = ctx
        .feed
        .game_data
        .players
        .get(&format!("ID{pitcher}"))
        .map(|p| p.full_name.clone())
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {full}"), theme::title()),
            Span::styled(format!("  {hand}"), theme::meta()),
        ]),
        Line::from(vec![Span::styled(
            format!(
                " {total} pitches · {strikes} strikes ({}) · CSW {} · {whiffs} whiffs on {swings} swings ({})",
                pct(strikes, total),
                pct(csw, total),
                pct(whiffs, swings)
            ),
            theme::text(),
        )]),
        Line::raw(""),
    ];
    let cols = COLS;
    let mut head = vec![Span::styled(pad(" Pitch", NAME_W), theme::meta())];
    for (c, w) in cols {
        head.push(Span::styled(rpad(c, w), theme::meta()));
    }
    lines.push(Line::from(head));
    for (code, t) in &rows {
        let velo = mean(&t.speed);
        let max = t.speed.iter().cloned().fold(f64::NAN, f64::max);
        let szn = season.iter().find(|s| &s.code == code).map(|s| s.mph);
        let (delta, delta_style) = match (velo, szn) {
            (Some(v), Some(s)) if s > 0.0 => {
                let d = v - s;
                let style = if d <= -1.0 {
                    theme::error()
                } else if d >= 1.0 {
                    theme::accent()
                } else {
                    theme::secondary()
                };
                (format!("{d:+.1}"), style)
            }
            _ => (String::new(), theme::meta()),
        };
        let f1 = |v: Option<f64>| v.map(|x| format!("{x:.1}")).unwrap_or_default();
        let f0 = |v: Option<f64>| v.map(|x| format!("{x:.0}")).unwrap_or_default();
        let vals: Vec<(String, Style)> = vec![
            (t.n.to_string(), theme::title()),
            (pct(t.n, total), theme::text()),
            (f1(velo), theme::title()),
            (
                if max.is_nan() {
                    String::new()
                } else {
                    format!("{max:.1}")
                },
                theme::secondary(),
            ),
            (delta, delta_style),
            (f0(mean(&t.spin)), theme::text()),
            (f1(mean(&t.ivb)), theme::text()),
            (f1(mean(&t.hb)), theme::text()),
            (pct(t.csw, t.n), theme::text()),
            (pct(t.whiffs, t.swings), theme::text()),
            (f1(mean(&t.evs)), theme::secondary()),
        ];
        let mut spans = vec![
            Span::styled(" ■ ", Style::new().fg(type_color(code))),
            Span::styled(pad(&short_name(&t.name), NAME_W - 3), theme::text()),
        ];
        for ((v, s), (_, w)) in vals.into_iter().zip(cols) {
            spans.push(Span::styled(rpad(&v, w), s));
        }
        lines.push(Line::from(spans));
    }

    // Velocity by inning for the two most-used pitches (fatigue shows here).
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " Velocity by inning",
        theme::accent2().add_modifier(Modifier::BOLD),
    ));
    let innings: Vec<u32> = {
        let mut v: Vec<u32> = mine.iter().map(|r| r.inning).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut head = vec![Span::styled(pad(" Inning", NAME_W), theme::meta())];
    for inn in &innings {
        head.push(Span::styled(rpad(&inn.to_string(), 6), theme::meta()));
    }
    lines.push(Line::from(head));
    for (code, t) in rows.iter().take(2) {
        let mut spans = vec![
            Span::styled(" ■ ", Style::new().fg(type_color(code))),
            Span::styled(pad(&short_name(&t.name), NAME_W - 3), theme::text()),
        ];
        for inn in &innings {
            let v: Vec<f64> = mine
                .iter()
                .filter(|r| &r.code == code && r.inning == *inn)
                .filter_map(|r| r.speed)
                .collect();
            spans.push(Span::styled(
                rpad(
                    &mean(&v)
                        .map(|m| format!("{m:.1}"))
                        .unwrap_or_else(|| "·".into()),
                    6,
                ),
                theme::title(),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        " Δszn: game velo minus season average (red: 1+ mph down)",
        theme::meta(),
    ));
    lines.push(Line::styled(
        " IVB, HB in inches; HB + = arm side · CSW: called strikes + whiffs",
        theme::meta(),
    ));
    if season.is_empty() && !mine.is_empty() {
        lines.push(Line::styled(" Season averages loading…", theme::meta()));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_movement(f: &mut Frame, mine: &[&PitchRec], area: Rect) {
    let block = panel("Movement", false)
        .title_bottom(Line::styled(" glove ← HB → arm · IVB ↑ ", theme::meta()));
    let inner = block.inner(area);
    // Same physical scale on both axes.
    let ratio = inner.width.max(1) as f64 / (2.0 * inner.height.max(1) as f64);
    let half_y = 25.0;
    let half_x = (half_y * ratio).max(25.0);
    let half_y = half_x / ratio;
    let mut groups: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for r in mine {
        if let (Some(x), Some(y)) = (r.hb_arm, r.ivb) {
            groups.entry(r.code.clone()).or_default().push((x, y));
        }
    }
    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .background_color(theme::BG0)
        .x_bounds([-half_x, half_x])
        .y_bounds([-half_y, half_y])
        .paint(move |c| {
            c.draw(&CLine::new(-half_x, 0.0, half_x, 0.0, theme::BG2));
            c.draw(&CLine::new(0.0, -half_y, 0.0, half_y, theme::BG2));
            for (code, pts) in &groups {
                c.draw(&Points {
                    coords: pts,
                    color: type_color(code),
                });
            }
            c.layer();
            for (code, pts) in &groups {
                let n = pts.len() as f64;
                let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
                let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
                c.print(
                    mx,
                    my,
                    Span::styled(
                        code.clone(),
                        Style::new()
                            .fg(theme::BG0)
                            .bg(type_color(code))
                            .add_modifier(Modifier::BOLD),
                    ),
                );
            }
        });
    f.render_widget(canvas, area);
}
