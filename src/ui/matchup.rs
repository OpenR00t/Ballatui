// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Batter vs pitcher at the shown moment: season lines, advanced stats on
//! bars against the league average, platoon splits, pitch mix (season and
//! this game) and their career head-to-head.

use super::gameview::Ctx;
use super::{pad, panel, rpad};
use crate::game::{hits_for, last_name};
use crate::model::Play;
use crate::stats::{
    ArsenalPitch, GroupStats, League, Load, PlayerStats, Stat, iso, num, pct1, per_pa, rate3,
    slash, text,
};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::collections::HashMap;

/// Odd, so the league-average tick sits exactly in the middle.
const BAR_W: usize = 17;

pub fn draw(f: &mut Frame, ctx: &Ctx, area: Rect) {
    let plays = &ctx.feed.live_data.plays.all_plays;
    let Some(play) = plays.get(ctx.cursor.play) else {
        super::message(
            f,
            area,
            "The matchup shows up once the game starts.",
            theme::meta(),
        );
        return;
    };
    let (b_id, p_id) = (play.matchup.batter.id, play.matchup.pitcher.id);
    let batter = ctx.stats.players.get(&(b_id, ctx.season));
    let pitcher = ctx.stats.players.get(&(p_id, ctx.season));
    let league = ctx.stats.league(ctx.season);

    let wide = area.width >= 120;
    let cols: Vec<Rect> = if wide {
        Layout::horizontal([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area)
        .to_vec()
    } else {
        Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(area)
            .to_vec()
    };

    // Batter.
    let title = format!(
        "Batter · {}",
        last_name(ctx.feed, b_id, &play.matchup.batter.full_name)
    );
    let mut lines = match batter {
        Some(Load::Ready(s)) => batter_lines(ctx, play, s, league, cols[0].width as usize),
        other => status_lines(other),
    };
    if !wide {
        lines.push(Line::raw(""));
        lines.extend(head_to_head_lines(ctx, play));
    }
    draw_card(f, &title, lines, cols[0], true);

    // Pitcher.
    let title = format!(
        "Pitcher · {}",
        last_name(ctx.feed, p_id, &play.matchup.pitcher.full_name)
    );
    let mut lines = match pitcher {
        Some(Load::Ready(s)) => pitcher_lines(ctx, play, s, league, cols[1].width as usize),
        other => status_lines(other),
    };
    if !wide {
        lines.push(Line::raw(""));
        lines.extend(mix_lines(ctx, play, cols[1].width as usize));
    }
    draw_card(f, &title, lines, cols[1], false);

    if wide {
        let mut lines = mix_lines(ctx, play, cols[2].width as usize);
        lines.push(Line::raw(""));
        lines.extend(head_to_head_lines(ctx, play));
        lines.push(Line::raw(""));
        for note in [
            format!(" Stats: {} regular season", ctx.season),
            " ┃ league average · green better · red worse".to_string(),
            " wOBA wRC+ FIP xFIP WAR: MLB's figures".to_string(),
            " xBA xSLG xwOBA: Statcast expected".to_string(),
        ] {
            lines.push(Line::styled(note, theme::meta()));
        }
        draw_card(f, "Pitch mix · head to head", lines, cols[2], false);
    }
}

fn draw_card(f: &mut Frame, title: &str, lines: Vec<Line>, area: Rect, focused: bool) {
    f.render_widget(Paragraph::new(lines).block(panel(title, focused)), area);
}

fn status_lines<T>(l: Option<&Load<T>>) -> Vec<Line<'static>> {
    match l {
        Some(Load::Failed(e)) => vec![Line::styled(
            format!(" Couldn't load stats: {e}"),
            theme::error(),
        )],
        _ => vec![Line::styled(" Loading stats…", theme::meta())],
    }
}

fn heading(t: &str) -> Line<'static> {
    Line::styled(
        format!(" {t}"),
        theme::accent2().add_modifier(Modifier::BOLD),
    )
}

/// A row of small column headers and their values underneath.
fn table(cols: &[(&str, String)]) -> Vec<Line<'static>> {
    let mut head = vec![Span::raw(" ")];
    let mut vals = vec![Span::raw(" ")];
    for (h, v) in cols {
        let w = h.len().max(v.chars().count()) + 2;
        head.push(Span::styled(rpad(h, w), theme::meta()));
        vals.push(Span::styled(rpad(v, w), theme::title()));
    }
    vec![Line::from(head), Line::from(vals)]
}

/// One stat on a bar centered on the league average.
struct BarSpec<'a> {
    label: &'a str,
    value: Option<f64>,
    avg: Option<f64>,
    /// Half the bar's range, in the stat's units.
    spread: f64,
    higher_better: bool,
    fmt: fn(f64) -> String,
}

fn bar_line(b: BarSpec) -> Line<'static> {
    let label = Span::styled(pad(&format!(" {}", b.label), 8), theme::secondary());
    let Some(v) = b.value else {
        return Line::from(vec![label, Span::styled(rpad("—", 7), theme::meta())]);
    };
    let mut spans = vec![
        label,
        Span::styled(rpad(&(b.fmt)(v), 7), theme::title()),
        Span::raw("  "),
    ];
    match b.avg {
        Some(avg) => {
            let half = (BAR_W / 2) as f64;
            let a = BAR_W / 2;
            let p = (half + ((v - avg) / b.spread).clamp(-1.0, 1.0) * half).round() as usize;
            let better = (v > avg) == b.higher_better;
            let near = (v - avg).abs() < b.spread * 0.04;
            let color = if near {
                theme::FG3
            } else if better {
                theme::GREEN
            } else {
                theme::RED
            };
            let (from, to) = (a.min(p), a.max(p));
            for i in 0..BAR_W {
                let (ch, style) = if i == a {
                    ("┃", Style::new().fg(theme::FG3))
                } else if i >= from && i <= to {
                    ("━", Style::new().fg(color))
                } else {
                    ("─", Style::new().fg(theme::BG2))
                };
                spans.push(Span::styled(ch, style));
            }
            spans.push(Span::styled(
                format!("  lg {}", (b.fmt)(avg)),
                theme::meta(),
            ));
        }
        None => spans.push(Span::styled("league avg loading…", theme::meta())),
    }
    Line::from(spans)
}

fn f_rate(v: f64) -> String {
    rate3(v)
}
fn f_pct(v: f64) -> String {
    pct1(v)
}
fn f_int(v: f64) -> String {
    format!("{v:.0}")
}
fn f_era(v: f64) -> String {
    format!("{v:.2}")
}

fn get(s: &Stat, k: &str) -> String {
    text(s, k).unwrap_or_else(|| "—".into())
}

/// Platoon split line, marked when it applies to this matchup.
fn split_line(label: &str, s: &Stat, active: bool) -> Line<'static> {
    let pa = num(s, "plateAppearances")
        .or_else(|| num(s, "battersFaced"))
        .map(|v| format!("{v:.0} PA"))
        .unwrap_or_default();
    let mark = if active { "▶" } else { " " };
    let style = if active {
        theme::title()
    } else {
        theme::text()
    };
    Line::from(vec![
        Span::styled(format!(" {mark} "), theme::accent()),
        Span::styled(
            pad(label, 8),
            if active {
                theme::accent()
            } else {
                theme::secondary()
            },
        ),
        Span::styled(pad(&slash(s).unwrap_or_else(|| "—".into()), 16), style),
        Span::styled(rpad(&pa, 8), theme::meta()),
    ])
}

/// The side a batter hits from against this pitcher (switch hitters take
/// the opposite side).
fn batting_side(bats: &str, throws: &str) -> &'static str {
    match (bats, throws) {
        ("S", "L") => "R",
        ("S", _) => "L",
        ("L", _) => "L",
        _ => "R",
    }
}

fn batter_lines(
    ctx: &Ctx,
    play: &Play,
    s: &PlayerStats,
    lg: Option<League>,
    _width: usize,
) -> Vec<Line<'static>> {
    let h: &GroupStats = &s.hitting;
    let mut out = vec![Line::from(vec![
        Span::styled(format!(" {}", s.name), theme::title()),
        Span::styled(format!("  {} · bats {}", s.position, s.bats), theme::meta()),
    ])];
    if h.is_empty() {
        out.push(Line::styled(
            format!(" No {} regular-season batting stats.", ctx.season),
            theme::meta(),
        ));
        return out;
    }
    out.push(Line::raw(""));
    let sea = &h.season;
    out.extend(table(&[
        ("PA", get(sea, "plateAppearances")),
        ("AVG", get(sea, "avg")),
        ("OBP", get(sea, "obp")),
        ("SLG", get(sea, "slg")),
        ("HR", get(sea, "homeRuns")),
        ("SB", get(sea, "stolenBases")),
    ]));
    out.push(Line::raw(""));
    out.push(heading("Advanced"));
    let a = |f: fn(&League) -> f64| lg.as_ref().map(f);
    out.push(bar_line(BarSpec {
        label: "wRC+",
        value: num(&h.saber, "wRcPlus"),
        avg: Some(100.0),
        spread: 70.0,
        higher_better: true,
        fmt: f_int,
    }));
    out.push(bar_line(BarSpec {
        label: "wOBA",
        value: num(&h.saber, "woba"),
        avg: a(League::woba),
        spread: 0.100,
        higher_better: true,
        fmt: f_rate,
    }));
    out.push(bar_line(BarSpec {
        label: "xwOBA",
        value: num(&h.expected, "woba"),
        avg: a(League::woba),
        spread: 0.100,
        higher_better: true,
        fmt: f_rate,
    }));
    out.push(bar_line(BarSpec {
        label: "ISO",
        value: iso(sea),
        avg: a(League::iso),
        spread: 0.150,
        higher_better: true,
        fmt: f_rate,
    }));
    out.push(bar_line(BarSpec {
        label: "K%",
        value: per_pa(sea, "strikeOuts"),
        avg: lg.map(|l| l.k_pct),
        spread: 0.15,
        higher_better: false,
        fmt: f_pct,
    }));
    out.push(bar_line(BarSpec {
        label: "BB%",
        value: per_pa(sea, "baseOnBalls"),
        avg: lg.map(|l| l.bb_pct),
        spread: 0.08,
        higher_better: true,
        fmt: f_pct,
    }));
    out.push(bar_line(BarSpec {
        label: "BABIP",
        value: num(sea, "babip"),
        avg: lg.map(|l| l.babip),
        spread: 0.080,
        higher_better: true,
        fmt: f_rate,
    }));
    let mut extra = Vec::new();
    if let Some(x) = text(&h.expected, "avg") {
        extra.push(format!("xBA {x}"));
    }
    if let Some(x) = text(&h.expected, "slg") {
        extra.push(format!("xSLG {x}"));
    }
    if let Some(w) = num(&h.saber, "war") {
        extra.push(format!("WAR {w:.1}"));
    }
    out.push(Line::styled(
        format!(" {}", extra.join(" · ")),
        theme::secondary(),
    ));
    out.push(Line::raw(""));
    out.push(heading("Platoon"));
    let throws = ctx
        .stats
        .player(play.matchup.pitcher.id, ctx.season)
        .map(|p| p.throws.clone())
        .unwrap_or_else(|| play.matchup.pitch_hand.code.clone());
    out.push(split_line("vs LHP", &h.vs_left, throws == "L"));
    out.push(split_line("vs RHP", &h.vs_right, throws == "R"));
    out.push(Line::raw(""));
    out.push(heading("This game"));
    let results = ctx
        .sit
        .batter_results
        .get(&play.matchup.batter.id)
        .cloned()
        .unwrap_or_default();
    let (hits, ab) = hits_for(&results);
    out.push(Line::styled(
        if results.is_empty() {
            " First time up".to_string()
        } else {
            format!(" {hits}-for-{ab} · {}", results.join(", "))
        },
        theme::text(),
    ));
    out
}

fn pitcher_lines(
    ctx: &Ctx,
    play: &Play,
    s: &PlayerStats,
    lg: Option<League>,
    _width: usize,
) -> Vec<Line<'static>> {
    let p: &GroupStats = &s.pitching;
    let mut out = vec![Line::from(vec![
        Span::styled(format!(" {}", s.name), theme::title()),
        Span::styled(format!("  {}HP", s.throws), theme::meta()),
    ])];
    if p.is_empty() {
        out.push(Line::styled(
            format!(" No {} regular-season pitching stats.", ctx.season),
            theme::meta(),
        ));
        return out;
    }
    out.push(Line::raw(""));
    let sea = &p.season;
    let wl = format!("{}-{}", get(sea, "wins"), get(sea, "losses"));
    out.extend(table(&[
        ("G", get(sea, "gamesPlayed")),
        ("W-L", wl),
        ("IP", get(sea, "inningsPitched")),
        ("ERA", get(sea, "era")),
        ("WHIP", get(sea, "whip")),
        ("SO", get(sea, "strikeOuts")),
    ]));
    out.push(Line::raw(""));
    out.push(heading("Advanced"));
    let era = lg.map(|l| l.era);
    out.push(bar_line(BarSpec {
        label: "FIP",
        value: num(&p.saber, "fip"),
        avg: era,
        spread: 2.0,
        higher_better: false,
        fmt: f_era,
    }));
    out.push(bar_line(BarSpec {
        label: "xFIP",
        value: num(&p.saber, "xfip"),
        avg: era,
        spread: 2.0,
        higher_better: false,
        fmt: f_era,
    }));
    out.push(bar_line(BarSpec {
        label: "xwOBA",
        value: num(&p.expected, "woba"),
        avg: lg.map(|l| l.woba()),
        spread: 0.080,
        higher_better: false,
        fmt: f_rate,
    }));
    let k = per_pa(sea, "strikeOuts");
    let bb = per_pa(sea, "baseOnBalls");
    out.push(bar_line(BarSpec {
        label: "K%",
        value: k,
        avg: lg.map(|l| l.k_pct),
        spread: 0.15,
        higher_better: true,
        fmt: f_pct,
    }));
    out.push(bar_line(BarSpec {
        label: "BB%",
        value: bb,
        avg: lg.map(|l| l.bb_pct),
        spread: 0.06,
        higher_better: false,
        fmt: f_pct,
    }));
    out.push(bar_line(BarSpec {
        label: "K-BB%",
        value: k.zip(bb).map(|(k, b)| k - b),
        avg: lg.map(|l| l.k_pct - l.bb_pct),
        spread: 0.15,
        higher_better: true,
        fmt: f_pct,
    }));
    let mut extra = Vec::new();
    if let Some(x) = text(sea, "homeRunsPer9") {
        extra.push(format!("HR/9 {x}"));
    }
    if let Some(x) = num(&p.saber, "eraMinus") {
        extra.push(format!("ERA- {x:.0}"));
    }
    if let Some(x) = num(&p.saber, "fipMinus") {
        extra.push(format!("FIP- {x:.0}"));
    }
    if let Some(w) = num(&p.saber, "war") {
        extra.push(format!("WAR {w:.1}"));
    }
    out.push(Line::styled(
        format!(" {}", extra.join(" · ")),
        theme::secondary(),
    ));
    out.push(Line::raw(""));
    out.push(heading("Platoon (opponents)"));
    let bats = ctx
        .stats
        .player(play.matchup.batter.id, ctx.season)
        .map(|b| b.bats.clone())
        .unwrap_or_else(|| play.matchup.bat_side.code.clone());
    let side = batting_side(&bats, &s.throws);
    out.push(split_line("vs LHB", &p.vs_left, side == "L"));
    out.push(split_line("vs RHB", &p.vs_right, side == "R"));
    out.push(Line::raw(""));
    out.push(heading("This game"));
    let t = ctx
        .sit
        .pitchers
        .get(&play.matchup.pitcher.id)
        .cloned()
        .unwrap_or_default();
    out.push(Line::styled(
        format!(
            " {}.{} IP · {} H · {} R · {} K · {} BB · {} P",
            t.outs / 3,
            t.outs % 3,
            t.hits,
            t.runs,
            t.strikeouts,
            t.walks,
            t.pitches
        ),
        theme::text(),
    ));
    out
}

/// This pitcher's pitch types in the game so far, by type code.
fn game_mix(ctx: &Ctx, pitcher: u64) -> (HashMap<String, (u32, String)>, u32) {
    let plays = &ctx.feed.live_data.plays.all_plays;
    let mut counts: HashMap<String, (u32, String)> = HashMap::new();
    let mut total = 0;
    for (i, p) in plays.iter().enumerate().take(ctx.cursor.play + 1) {
        if p.matchup.pitcher.id != pitcher {
            continue;
        }
        let shown = if i < ctx.cursor.play {
            p.play_events.len()
        } else {
            ctx.cursor.events.min(p.play_events.len())
        };
        for e in &p.play_events[..shown] {
            if let (true, Some(t)) = (e.is_pitch, e.details.pitch_type.as_ref()) {
                let entry = counts
                    .entry(t.code.clone())
                    .or_insert((0, t.description.clone()));
                entry.0 += 1;
                total += 1;
            }
        }
    }
    (counts, total)
}

fn mix_lines(ctx: &Ctx, play: &Play, width: usize) -> Vec<Line<'static>> {
    let pid = play.matchup.pitcher.id;
    let mut out = vec![heading("Pitch mix")];
    let season: Vec<ArsenalPitch> = match ctx.stats.arsenal.get(&(pid, ctx.season)) {
        Some(Load::Ready(a)) => a.clone(),
        Some(Load::Failed(_)) => Vec::new(),
        _ => {
            out.push(Line::styled(" Loading…", theme::meta()));
            return out;
        }
    };
    let (game, total) = game_mix(ctx, pid);
    let name_w = 16;
    let bar_w = width.saturating_sub(name_w + 26).clamp(4, 14);
    out.push(Line::from(vec![
        Span::styled(pad("", name_w + 1), theme::meta()),
        Span::styled(rpad("season", 7), theme::meta()),
        Span::raw(" "),
        Span::styled(pad("", bar_w), theme::meta()),
        Span::styled(rpad("today", 6), theme::meta()),
        Span::styled(rpad("mph", 6), theme::meta()),
    ]));
    let max = season.iter().map(|p| p.share).fold(0.0, f64::max).max(0.01);
    // Season pitches first, then anything thrown today that isn't in the
    // season mix.
    let mut rows: Vec<(String, String, Option<f64>, Option<f64>)> = season
        .iter()
        .map(|p| (p.code.clone(), p.name.clone(), Some(p.share), Some(p.mph)))
        .collect();
    for (code, (_, name)) in &game {
        if !rows.iter().any(|r| &r.0 == code) {
            rows.push((code.clone(), name.clone(), None, None));
        }
    }
    for (code, name, share, mph) in rows {
        let today = game
            .get(&code)
            .map(|(n, _)| format!("{:.0}%", *n as f64 * 100.0 / total.max(1) as f64))
            .unwrap_or_else(|| "·".into());
        let filled = share
            .map(|s| ((s / max) * bar_w as f64).round() as usize)
            .unwrap_or(0);
        out.push(Line::from(vec![
            Span::styled(pad(&format!(" {name}"), name_w + 1), theme::text()),
            Span::styled(
                rpad(
                    &share
                        .map(|s| format!("{:.0}%", s * 100.0))
                        .unwrap_or_default(),
                    7,
                ),
                theme::title(),
            ),
            Span::raw(" "),
            Span::styled("█".repeat(filled), Style::new().fg(theme::AQUA)),
            Span::styled(" ".repeat(bar_w - filled.min(bar_w)), Style::new()),
            Span::styled(rpad(&today, 6), theme::accent()),
            Span::styled(
                rpad(&mph.map(|m| format!("{m:.1}")).unwrap_or_default(), 6),
                theme::secondary(),
            ),
        ]));
    }
    if season.is_empty() && game.is_empty() {
        out.push(Line::styled(" No pitch data.", theme::meta()));
    }
    out.push(Line::styled(
        format!(" today: {total} pitches so far"),
        theme::meta(),
    ));
    out
}

fn head_to_head_lines(ctx: &Ctx, play: &Play) -> Vec<Line<'static>> {
    let (b, p) = (play.matchup.batter.id, play.matchup.pitcher.id);
    let mut out = vec![heading(&format!(
        "{} vs {}, career",
        last_name(ctx.feed, b, &play.matchup.batter.full_name),
        last_name(ctx.feed, p, &play.matchup.pitcher.full_name)
    ))];
    match ctx.stats.head_to_head.get(&(b, p)) {
        Some(Load::Ready(Some(s))) if num(s, "plateAppearances").unwrap_or(0.0) > 0.0 => {
            let n = |k: &str| num(s, k).unwrap_or(0.0) as u32;
            let mut parts = vec![format!("{}-for-{}", n("hits"), n("atBats"))];
            for (k, label) in [
                ("doubles", "2B"),
                ("triples", "3B"),
                ("homeRuns", "HR"),
                ("baseOnBalls", "BB"),
                ("strikeOuts", "K"),
            ] {
                if n(k) > 0 {
                    parts.push(format!("{} {label}", n(k)));
                }
            }
            out.push(Line::styled(
                format!(" {}", parts.join(" · ")),
                theme::title(),
            ));
            out.push(Line::styled(
                format!(
                    " {}  ·  {} PA",
                    slash(s).unwrap_or_default(),
                    n("plateAppearances")
                ),
                theme::secondary(),
            ));
        }
        Some(Load::Ready(_)) => out.push(Line::styled(" First career meeting.", theme::text())),
        Some(Load::Failed(e)) => {
            out.push(Line::styled(format!(" Couldn't load: {e}"), theme::error()))
        }
        _ => out.push(Line::styled(" Loading…", theme::meta())),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::batting_side;

    #[test]
    fn switch_hitters_bat_opposite() {
        assert_eq!(batting_side("S", "L"), "R");
        assert_eq!(batting_side("S", "R"), "L");
        assert_eq!(batting_side("L", "L"), "L");
        assert_eq!(batting_side("R", "L"), "R");
    }
}
