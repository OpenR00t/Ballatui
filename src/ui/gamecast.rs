// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! The live view: strike zone, situation, current at-bat and play log.

use super::gameview::Ctx;
use super::{ordinal, pad, panel, rpad};
use crate::game::{PitcherTally, hits_for, last_name};
use crate::model::{GameFeed, Play};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CLine};
use ratatui::widgets::{Paragraph, Wrap};

pub fn draw(f: &mut Frame, ctx: &Ctx, scroll: u16, area: Rect) {
    let plays = &ctx.feed.live_data.plays.all_plays;
    if plays.is_empty() {
        draw_preview(f, ctx, area);
        return;
    }
    let top_h = if area.height >= 26 {
        15
    } else {
        area.height.saturating_sub(6).clamp(9, 15)
    };
    let [top, log] = Layout::vertical([Constraint::Length(top_h), Constraint::Min(3)]).areas(area);
    if area.width >= 100 {
        let [zone, sit, ab] = Layout::horizontal([
            Constraint::Length(30),
            Constraint::Length(40),
            Constraint::Min(30),
        ])
        .areas(top);
        draw_zone(f, ctx, zone);
        draw_situation(f, ctx, sit);
        draw_at_bat(f, ctx, ab);
    } else {
        let [zone, sit] =
            Layout::horizontal([Constraint::Length(26), Constraint::Min(30)]).areas(top);
        draw_zone(f, ctx, zone);
        draw_situation(f, ctx, sit);
    }
    draw_log(f, ctx, scroll, log);
}

fn current_play<'a>(ctx: &Ctx<'a>) -> Option<&'a Play> {
    ctx.feed.live_data.plays.all_plays.get(ctx.cursor.play)
}

fn draw_zone(f: &mut Frame, ctx: &Ctx, area: Rect) {
    let play = current_play(ctx);
    let shown = play.map(|p| &p.play_events[..ctx.cursor.events.min(p.play_events.len())]);
    let pitches: Vec<_> = shown.into_iter().flatten().filter(|e| e.is_pitch).collect();
    let (top, bot) = pitches
        .iter()
        .rev()
        .find_map(|e| {
            let p = e.pitch_data.as_ref()?;
            Some((p.strike_zone_top?, p.strike_zone_bottom?))
        })
        .unwrap_or((3.4, 1.6));
    let half_plate = 17.0 / 24.0;
    let canvas = Canvas::default()
        .block(panel("Zone", false).title_bottom(Line::styled(" catcher's view ", theme::meta())))
        .marker(Marker::Braille)
        .background_color(theme::BG0)
        .x_bounds([-1.75, 1.75])
        .y_bounds([0.5, 4.5])
        .paint(move |c| {
            let edge = theme::FG4;
            let grid = theme::BG2;
            let third_w = 2.0 * half_plate / 3.0;
            let third_h = (top - bot) / 3.0;
            for k in 1..3 {
                let x = -half_plate + third_w * k as f64;
                c.draw(&CLine::new(x, bot, x, top, grid));
                let y = bot + third_h * k as f64;
                c.draw(&CLine::new(-half_plate, y, half_plate, y, grid));
            }
            c.draw(&CLine::new(-half_plate, bot, half_plate, bot, edge));
            c.draw(&CLine::new(-half_plate, top, half_plate, top, edge));
            c.draw(&CLine::new(-half_plate, bot, -half_plate, top, edge));
            c.draw(&CLine::new(half_plate, bot, half_plate, top, edge));
            // Home plate.
            c.draw(&CLine::new(-half_plate, 0.75, half_plate, 0.75, theme::BG3));
            c.layer();
            let n = pitches.len();
            for (i, e) in pitches.iter().enumerate() {
                let Some(pd) = e.pitch_data.as_ref() else {
                    continue;
                };
                let (Some(x), Some(z)) = (pd.coordinates.px, pd.coordinates.pz) else {
                    continue;
                };
                let code = e
                    .details
                    .call
                    .as_ref()
                    .map(|c| c.code.as_str())
                    .unwrap_or("");
                let mut style = Style::new()
                    .fg(theme::pitch_color(code))
                    .add_modifier(Modifier::BOLD);
                if i + 1 == n {
                    style = style.bg(theme::BG2);
                }
                let label = e.pitch_number.unwrap_or(i as u32 + 1).to_string();
                c.print(
                    x.clamp(-1.6, 1.5),
                    z.clamp(0.6, 4.4),
                    Span::styled(label, style),
                );
            }
        });
    f.render_widget(canvas, area);
}

fn dots(filled: u32, total: u32, color: ratatui::style::Color) -> Vec<Span<'static>> {
    (0..total)
        .map(|i| {
            if i < filled {
                Span::styled("● ", Style::new().fg(color))
            } else {
                Span::styled("○ ", Style::new().fg(theme::BG3))
            }
        })
        .collect()
}

fn base_glyph(on: bool) -> Span<'static> {
    if on {
        Span::styled("◆", theme::accent_bold())
    } else {
        Span::styled("◇", Style::new().fg(theme::BG4))
    }
}

fn innings_pitched(outs: u32) -> String {
    format!("{}.{}", outs / 3, outs % 3)
}

fn draw_situation(f: &mut Frame, ctx: &Ctx, area: Rect) {
    let block = panel("Situation", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let s = &ctx.sit;
    let feed = ctx.feed;
    let name =
        |id: Option<u64>| -> String { id.map(|i| last_name(feed, i, "")).unwrap_or_default() };
    let mut lines = Vec::new();
    let arrow = if s.top { "▲" } else { "▼" };
    let batting = if s.top {
        &feed.game_data.teams.away.abbreviation
    } else {
        &feed.game_data.teams.home.abbreviation
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(
                " {arrow} {} {}",
                if s.top { "Top" } else { "Bottom" },
                ordinal(s.inning)
            ),
            theme::accent_bold(),
        ),
        Span::styled(format!("  · {batting} batting"), theme::meta()),
    ]));
    // Diamond, second base on top, third left, first right.
    let c = 16usize;
    let n2 = name(s.bases[1]);
    let n3 = name(s.bases[2]);
    let n1 = name(s.bases[0]);
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(c)),
        base_glyph(s.bases[1].is_some()),
        Span::styled(format!(" {n2}"), theme::text()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(rpad(&n3, c - 5), theme::text()),
        Span::raw(" "),
        base_glyph(s.bases[2].is_some()),
        Span::raw("       "),
        base_glyph(s.bases[0].is_some()),
        Span::styled(format!(" {n1}"), theme::text()),
    ]));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(c)),
        Span::styled("⌂", Style::new().fg(theme::FG4)),
    ]));
    let mut count = vec![Span::styled(" B ", theme::meta())];
    count.extend(dots(s.balls.min(3), 3, theme::BLUE));
    count.push(Span::styled(" S ", theme::meta()));
    count.extend(dots(s.strikes.min(2), 2, theme::RED));
    count.push(Span::styled(" O ", theme::meta()));
    count.extend(dots(s.outs.min(2), 2, theme::FG0));
    if s.outs >= 3 {
        count.push(Span::styled("3 outs", theme::secondary()));
    }
    lines.push(Line::from(count));
    lines.push(Line::raw(""));

    if let Some(play) = current_play(ctx) {
        let m = &play.matchup;
        let pitching_box = feed.live_data.boxscore.teams.get(play.about.is_top_inning);
        let batting_box = feed.live_data.boxscore.teams.get(!play.about.is_top_inning);
        // Pitcher.
        let t: PitcherTally = s.pitchers.get(&m.pitcher.id).cloned().unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(" P  ", theme::meta()),
            Span::styled(m.pitcher.full_name.clone(), theme::title()),
            Span::styled(format!(" ({}HP)", m.pitch_hand.code), theme::meta()),
        ]));
        let era = pitching_box
            .player(m.pitcher.id)
            .and_then(|p| p.season_stats.pitching.era.clone())
            .map(|e| format!(" · {e} ERA"))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!(
                    "{} IP {} H {} R {} K {} BB",
                    innings_pitched(t.outs),
                    t.hits,
                    t.runs,
                    t.strikeouts,
                    t.walks
                ),
                theme::text(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{} pitches, {} strikes", t.pitches, t.strikes),
                theme::secondary(),
            ),
            Span::styled(era, theme::meta()),
        ]));
        // Batter.
        let results = s
            .batter_results
            .get(&m.batter.id)
            .cloned()
            .unwrap_or_default();
        let (h, ab) = hits_for(&results);
        let slot = batting_box
            .player(m.batter.id)
            .and_then(|p| p.lineup_slot())
            .map(|(n, _)| format!(" #{n}"))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(" AB ", theme::meta()),
            Span::styled(m.batter.full_name.clone(), theme::title()),
            Span::styled(format!(" ({}){slot}", m.bat_side.code), theme::meta()),
        ]));
        let today = if results.is_empty() {
            "first time up".to_string()
        } else {
            format!("{h}-for-{ab} · {}", results.join(", "))
        };
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(today, theme::text()),
        ]));
        if let Some(bs) = batting_box
            .player(m.batter.id)
            .map(|p| &p.season_stats.batting)
            && let (Some(avg), Some(obp), Some(slg)) = (&bs.avg, &bs.obp, &bs.slg)
        {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("{avg}/{obp}/{slg} · {} HR", bs.home_runs.unwrap_or(0)),
                    theme::meta(),
                ),
            ]));
        }
        if ctx.at_end
            && ctx.feed.game_data.status.is_live()
            && let Some(o) = &ctx.feed.live_data.linescore.offense
        {
            let od = o
                .on_deck
                .as_ref()
                .map(|p| last_name(feed, p.id, &p.full_name));
            let ih = o
                .in_hole
                .as_ref()
                .map(|p| last_name(feed, p.id, &p.full_name));
            if let (Some(od), Some(ih)) = (od, ih) {
                lines.push(Line::from(vec![
                    Span::styled(" Due ", theme::meta()),
                    Span::styled(format!("{od}, {ih}"), theme::secondary()),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_at_bat(f: &mut Frame, ctx: &Ctx, area: Rect) {
    let Some(play) = current_play(ctx) else {
        return;
    };
    let m = &play.matchup;
    let title = format!(
        "At-bat · {} vs {}",
        last_name(ctx.feed, m.batter.id, &m.batter.full_name),
        last_name(ctx.feed, m.pitcher.id, &m.pitcher.full_name)
    );
    let block = panel(&title, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let shown = &play.play_events[..ctx.cursor.events.min(play.play_events.len())];
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    for e in shown {
        if e.is_pitch {
            let code = e
                .details
                .call
                .as_ref()
                .map(|c| c.code.as_str())
                .unwrap_or("");
            let call = e
                .details
                .call
                .as_ref()
                .map(|c| c.description.clone())
                .or_else(|| e.details.description.clone())
                .unwrap_or_default();
            let kind = e
                .details
                .pitch_type
                .as_ref()
                .map(|t| t.description.clone())
                .unwrap_or_default();
            let mph = e
                .pitch_data
                .as_ref()
                .and_then(|p| p.start_speed)
                .map(|s| format!("{s:.1}"))
                .unwrap_or_default();
            let n = e.pitch_number.map(|n| n.to_string()).unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(
                    rpad(&n, 3),
                    Style::new()
                        .fg(theme::pitch_color(code))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(pad(&kind, 19), theme::text()),
                Span::styled(rpad(&mph, 5), theme::secondary()),
                Span::styled(" mph  ", theme::meta()),
                Span::styled(
                    pad(&call, w.saturating_sub(39)),
                    Style::new().fg(theme::pitch_color(code)),
                ),
                Span::styled(
                    format!(" {}-{}", e.count.balls, e.count.strikes),
                    theme::meta(),
                ),
            ]));
            if let Some(h) = &e.hit_data {
                let mut parts = Vec::new();
                if let Some(v) = h.launch_speed {
                    parts.push(format!("{v:.1} mph EV"));
                }
                if let Some(a) = h.launch_angle {
                    parts.push(format!("{a:.0}° LA"));
                }
                if let Some(d) = h.total_distance {
                    parts.push(format!("{d:.0} ft"));
                }
                if !parts.is_empty() {
                    lines.push(Line::styled(
                        format!("     {}", parts.join(" · ")),
                        theme::accent2(),
                    ));
                }
            }
        } else if let Some(d) = &e.details.description {
            let et = e.details.event_type.as_deref().unwrap_or("");
            if et == "batter_timeout" {
                continue;
            }
            lines.push(Line::styled(format!("   · {}", d.trim()), theme::meta()));
        }
    }
    let done = ctx.cursor.events >= play.play_events.len() && play.about.is_complete;
    if shown.is_empty() {
        lines.push(Line::styled("  Batter stepping in", theme::meta()));
    }
    // Keep the newest pitches in view, leaving room for the result.
    let result_lines: Vec<Line> = if done {
        let desc = play.result.description.clone().unwrap_or_default();
        let style = if play.about.is_scoring_play {
            theme::accent_bold()
        } else {
            theme::title()
        };
        vec![Line::raw(""), Line::styled(desc, style)]
    } else {
        Vec::new()
    };
    let room = inner.height as usize;
    let result_h = if done {
        1 + wrapped_height(&play.result.description.clone().unwrap_or_default(), w)
    } else {
        0
    };
    let keep = room.saturating_sub(result_h).max(1);
    if lines.len() > keep {
        lines.drain(..lines.len() - keep);
    }
    lines.extend(result_lines);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn wrapped_height(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let mut lines = 1;
    let mut col = 0;
    for word in text.split_whitespace() {
        let w = unicode_width::UnicodeWidthStr::width(word);
        if col > 0 && col + 1 + w > width {
            lines += 1;
            col = w;
        } else {
            col += w + usize::from(col > 0);
        }
    }
    lines
}

fn score_text(feed: &GameFeed, score: (u32, u32)) -> String {
    format!(
        "{} {}, {} {}",
        feed.game_data.teams.away.abbreviation,
        score.0,
        feed.game_data.teams.home.abbreviation,
        score.1
    )
}

fn draw_log(f: &mut Frame, ctx: &Ctx, scroll: u16, area: Rect) {
    let block = panel("Plays", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let log = crate::game::play_log(&ctx.feed.live_data.plays.all_plays, ctx.cursor);
    let mut lines = Vec::new();
    for l in log.iter().rev() {
        let tag = format!("{}{:<2}", if l.top { "▲" } else { "▼" }, l.inning);
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
            Span::styled(format!(" {tag} "), theme::accent2()),
            Span::styled(pad(&outs, 6), theme::meta()),
            Span::styled(l.text.clone(), style),
        ];
        if let Some(s) = l.score {
            spans.push(Span::styled(
                format!("  {}", score_text(ctx.feed, s)),
                theme::accent_bold(),
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inner,
    );
}

fn draw_preview(f: &mut Frame, ctx: &Ctx, area: Rect) {
    let gd = &ctx.feed.game_data;
    let block = panel("Pre-game", true);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" First pitch  ", theme::meta()),
            Span::styled(super::local_time(&gd.datetime.date_time), theme::title()),
            Span::styled(
                format!("  ·  {}", gd.venue.name.clone().unwrap_or_default()),
                theme::secondary(),
            ),
        ]),
        Line::raw(""),
        Line::styled(" Probable pitchers", theme::accent_bold()),
    ];
    for home in [false, true] {
        let team = gd.teams.get(home);
        let pp = gd
            .probable_pitchers
            .as_ref()
            .and_then(|p| p.get(home).clone());
        let (name, line) = match pp {
            Some(p) => {
                let stats = ctx
                    .feed
                    .live_data
                    .boxscore
                    .teams
                    .get(home)
                    .player(p.id)
                    .map(|b| &b.season_stats.pitching);
                let line = stats
                    .map(|s| {
                        format!(
                            "{}-{} · {} ERA · {} IP · {} K",
                            s.wins.unwrap_or(0),
                            s.losses.unwrap_or(0),
                            s.era.clone().unwrap_or_else(|| "-.--".into()),
                            s.innings_pitched.clone().unwrap_or_else(|| "0.0".into()),
                            s.strike_outs.unwrap_or(0)
                        )
                    })
                    .unwrap_or_default();
                (p.full_name.clone(), line)
            }
            None => ("TBD".into(), String::new()),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("   {} ", pad(&team.abbreviation, 4)),
                theme::accent2(),
            ),
            Span::styled(pad(&name, 24), theme::title()),
            Span::styled(line, theme::meta()),
        ]));
    }
    // Lineups once they're posted.
    let bx = &ctx.feed.live_data.boxscore.teams;
    if !bx.away.batting_order.is_empty() || !bx.home.batting_order.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(" Lineups", theme::accent_bold()));
        let col = (inner.width as usize / 2).max(30);
        for i in 0..9 {
            let mut spans = Vec::new();
            for home in [false, true] {
                let t = bx.get(home);
                let cell = t
                    .batting_order
                    .get(i)
                    .and_then(|id| t.player(*id))
                    .map(|p| {
                        let avg = p.season_stats.batting.avg.clone().unwrap_or_default();
                        format!(
                            "{} {} {}  {avg}",
                            i + 1,
                            pad(&p.person.full_name, 22),
                            pad(&p.position.abbreviation, 3)
                        )
                    })
                    .unwrap_or_default();
                spans.push(Span::styled(pad(&format!("   {cell}"), col), theme::text()));
            }
            lines.push(Line::from(spans));
        }
    }
    if let Some(w) = &gd.weather {
        lines.push(Line::raw(""));
        let parts: Vec<String> = [
            w.temp.as_ref().map(|t| format!("{t}°F")),
            w.condition.clone(),
            w.wind.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !parts.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(" Weather  ", theme::meta()),
                Span::styled(parts.join(" · "), theme::text()),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}
