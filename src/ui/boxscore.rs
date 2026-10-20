// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Box score for one team: batting, notes, pitching and game info. Always
//! the official numbers for the game so far (not rewound in history).

use super::gameview::Ctx;
use super::{opt, pad, panel, rpad};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub fn draw(f: &mut Frame, ctx: &Ctx, home: bool, scroll: u16, area: Rect) {
    let feed = ctx.feed;
    let team = feed.live_data.boxscore.teams.get(home);
    let info = feed.game_data.teams.get(home);
    let other = feed.game_data.teams.get(!home);
    let title = format!("{}  ·  t: {}", info.name, other.abbreviation);
    let block = panel(&title, true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    if !ctx.at_end {
        lines.push(Line::styled(
            " Box score shows the game so far, not the replay position.",
            theme::meta(),
        ));
        lines.push(Line::raw(""));
    }
    let name_w = 26;
    let cols = ["AB", "R", "H", "RBI", "BB", "K", "LOB", "AVG", "OPS"];
    let widths = [4, 4, 4, 4, 4, 4, 4, 6, 6];
    let mut head = vec![Span::styled(pad(" Batters", name_w), theme::accent_bold())];
    for (c, w) in cols.iter().zip(widths) {
        head.push(Span::styled(rpad(c, w), theme::accent()));
    }
    lines.push(Line::from(head));
    for id in &team.batters {
        let Some(p) = team.player(*id) else { continue };
        let Some((_, sub)) = p.lineup_slot() else {
            continue;
        };
        let b = &p.stats.batting;
        let s = &p.season_stats.batting;
        let pos: Vec<&str> = p
            .all_positions
            .iter()
            .map(|x| x.abbreviation.as_str())
            .collect();
        let name = format!(
            "{}{} {}",
            if sub { "   " } else { " " },
            feed.game_data
                .players
                .get(&format!("ID{id}"))
                .map(|i| i.boxscore_name.clone())
                .unwrap_or_else(|| p.person.full_name.clone()),
            pos.join("-")
        );
        let vals = [
            opt(b.at_bats),
            opt(b.runs),
            opt(b.hits),
            opt(b.rbi),
            opt(b.base_on_balls),
            opt(b.strike_outs),
            opt(b.left_on_base),
            s.avg.clone().unwrap_or_default(),
            s.ops.clone().unwrap_or_default(),
        ];
        let mut spans = vec![Span::styled(
            pad(&name, name_w),
            if sub {
                theme::secondary()
            } else {
                theme::text()
            },
        )];
        for (i, (v, w)) in vals.iter().zip(widths).enumerate() {
            let style = if i >= 7 {
                theme::meta()
            } else if v != "0" && (i == 2 || i == 1 || i == 3) {
                theme::title()
            } else {
                theme::text()
            };
            spans.push(Span::styled(rpad(v, w), style));
        }
        lines.push(Line::from(spans));
    }
    let t = &team.team_stats.batting;
    let mut tot = vec![Span::styled(pad(" Totals", name_w), theme::accent2())];
    for (v, w) in [
        opt(t.at_bats),
        opt(t.runs),
        opt(t.hits),
        opt(t.rbi),
        opt(t.base_on_balls),
        opt(t.strike_outs),
        opt(t.left_on_base),
    ]
    .iter()
    .zip(widths)
    {
        tot.push(Span::styled(
            rpad(v, w),
            theme::accent2().add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::from(tot));
    lines.push(Line::raw(""));

    // Batting / baserunning / fielding notes.
    for note in &team.note {
        lines.push(Line::from(vec![
            Span::styled(format!(" {}-", note.label), theme::meta()),
            Span::styled(note.value.clone().unwrap_or_default(), theme::secondary()),
        ]));
    }
    for sec in &team.info {
        lines.push(Line::styled(format!(" {}", sec.title), theme::accent2()));
        for lv in &sec.field_list {
            lines.push(Line::from(vec![
                Span::styled(format!("   {}: ", lv.label), theme::secondary()),
                Span::styled(lv.value.clone().unwrap_or_default(), theme::text()),
            ]));
        }
    }
    lines.push(Line::raw(""));

    let pcols = ["IP", "H", "R", "ER", "BB", "K", "HR", "PC-ST", "ERA"];
    let pw = [5, 4, 4, 4, 4, 4, 4, 8, 7];
    let mut head = vec![Span::styled(pad(" Pitchers", name_w), theme::accent_bold())];
    for (c, w) in pcols.iter().zip(pw) {
        head.push(Span::styled(rpad(c, w), theme::accent()));
    }
    lines.push(Line::from(head));
    for id in &team.pitchers {
        let Some(p) = team.player(*id) else { continue };
        let s = &p.stats.pitching;
        let name = format!(
            " {} {}",
            feed.game_data
                .players
                .get(&format!("ID{id}"))
                .map(|i| i.boxscore_name.clone())
                .unwrap_or_else(|| p.person.full_name.clone()),
            s.note.clone().unwrap_or_default()
        );
        let pc = format!(
            "{}-{}",
            s.number_of_pitches.or(s.pitches_thrown).unwrap_or(0),
            s.strikes.unwrap_or(0)
        );
        let vals = [
            s.innings_pitched.clone().unwrap_or_default(),
            opt(s.hits),
            opt(s.runs),
            opt(s.earned_runs),
            opt(s.base_on_balls),
            opt(s.strike_outs),
            opt(s.home_runs),
            pc,
            p.season_stats.pitching.era.clone().unwrap_or_default(),
        ];
        let mut spans = vec![Span::styled(pad(&name, name_w), theme::text())];
        for (i, (v, w)) in vals.iter().zip(pw).enumerate() {
            spans.push(Span::styled(
                rpad(v, w),
                if i == 8 { theme::meta() } else { theme::text() },
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    let game_info = &feed.live_data.boxscore.info;
    if !game_info.is_empty() {
        lines.push(Line::styled(" Game", theme::accent_bold()));
        for lv in game_info {
            match &lv.value {
                Some(v) => lines.push(Line::from(vec![
                    Span::styled(format!("   {}: ", lv.label), theme::secondary()),
                    Span::styled(v.clone(), theme::text()),
                ])),
                None => lines.push(Line::styled(format!("   {}", lv.label), theme::meta())),
            }
        }
    }
    let max = (lines.len() as u16).saturating_sub(inner.height / 2);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(max), 0)),
        inner,
    );
}
