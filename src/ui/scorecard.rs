// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! A traditional scorebook page for one team, filled in up to the shown
//! moment: lineup down the side, innings across, one box per plate
//! appearance with its notation and how far the batter got around.

use super::gameview::Ctx;
use super::{cpad, pad, panel};
use crate::game::{CellKind, PaCell, scorecard};
use crate::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const NAME_W: usize = 20;
const CELL_W: usize = 9;

pub fn draw(f: &mut Frame, ctx: &Ctx, home: bool, scroll: u16, hscroll: u16, area: Rect) {
    let feed = ctx.feed;
    let info = feed.game_data.teams.get(home);
    let other = feed.game_data.teams.get(!home);
    let block = panel(
        &format!("Scorecard · {}  ·  t: {}", info.name, other.abbreviation),
        true,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let card = scorecard(feed, home, Some(ctx.cursor));
    let innings = card.innings as usize;
    let rule_style = Style::new().fg(theme::BG3);
    let sep = |left: &str, mid: &str, right: &str| -> Line<'static> {
        let mut s = String::new();
        s.push_str(&"─".repeat(NAME_W));
        s.push_str(left);
        for i in 0..innings {
            s.push_str(&"─".repeat(CELL_W - 1));
            s.push_str(if i + 1 == innings { right } else { mid });
        }
        Line::styled(s, rule_style)
    };

    let mut lines = Vec::new();
    let mut head = vec![
        Span::styled(pad("", NAME_W), theme::meta()),
        Span::styled("│", rule_style),
    ];
    for i in 1..=innings {
        let current = ctx.sit.inning as usize == i && ctx.sit.top != home;
        let style = if current {
            theme::accent_bold()
        } else {
            theme::accent()
        };
        head.push(Span::styled(cpad(&i.to_string(), CELL_W - 1), style));
        head.push(Span::styled("│", rule_style));
    }
    lines.push(Line::from(head));
    lines.push(sep("┼", "┼", "┤"));

    for (slot, row) in card.rows.iter().enumerate() {
        let mut l1 = vec![];
        let mut l2 = vec![];
        let starter = row.players.first();
        let n1 = starter
            .map(|(_, n, p)| format!("{} {n} {p}", slot + 1))
            .unwrap_or_else(|| format!("{}", slot + 1));
        let n2 = row
            .players
            .iter()
            .skip(1)
            .map(|(_, n, p)| format!("{n} {p}"))
            .collect::<Vec<_>>()
            .join(", ");
        l1.push(Span::styled(pad(&n1, NAME_W), theme::text()));
        l2.push(Span::styled(
            pad(&format!("  {n2}"), NAME_W),
            theme::secondary(),
        ));
        l1.push(Span::styled("│", rule_style));
        l2.push(Span::styled("│", rule_style));
        for i in 1..=innings as u32 {
            let cells = row.cells.get(&i).map(|v| v.as_slice()).unwrap_or(&[]);
            let (a, b) = cell_spans(cells, ctx.backwards_k);
            l1.extend(a);
            l2.extend(b);
            l1.push(Span::styled("│", rule_style));
            l2.push(Span::styled("│", rule_style));
        }
        lines.push(Line::from(l1));
        lines.push(Line::from(l2));
        lines.push(sep("┼", "┼", "┤"));
    }
    // Inning totals.
    let mut tr = vec![
        Span::styled(pad(" R / H", NAME_W), theme::accent2()),
        Span::styled("│", rule_style),
    ];
    for i in 1..=innings as u32 {
        let t = card
            .totals
            .get(&i)
            .map(|(r, h)| format!("{r}/{h}"))
            .unwrap_or_default();
        tr.push(Span::styled(cpad(&t, CELL_W - 1), theme::accent2()));
        tr.push(Span::styled("│", rule_style));
    }
    lines.push(Line::from(tr));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" Key  ", theme::meta()),
        Span::styled("● ", theme::accent()),
        Span::styled("bases reached  ", theme::meta()),
        Span::styled("◆ ", theme::accent_bold()),
        Span::styled("run  ", theme::meta()),
        Span::styled("• ", Style::new().fg(theme::ORANGE)),
        Span::styled("RBI  ", theme::meta()),
        Span::styled("✕2 ", theme::error()),
        Span::styled("out at 2nd  ", theme::meta()),
        Span::styled("①②③ ", theme::secondary()),
        Span::styled("out number  ", theme::meta()),
        Span::styled(
            if ctx.backwards_k {
                format!("{BACKWARDS_K} ")
            } else {
                "KL ".to_string()
            },
            theme::text(),
        ),
        Span::styled("called third strike  ", theme::meta()),
        Span::styled("AR ", theme::text()),
        Span::styled("automatic runner", theme::meta()),
    ]));

    let max = (lines.len() as u16).saturating_sub(inner.height);
    f.render_widget(
        Paragraph::new(lines).scroll((scroll.min(max), hscroll)),
        inner,
    );
}

fn kind_style(k: CellKind) -> Style {
    match k {
        CellKind::Hit => theme::accent_bold(),
        CellKind::Walk => theme::accent2(),
        CellKind::Strikeout => Style::new().fg(theme::FG0).add_modifier(Modifier::BOLD),
        CellKind::Reached => Style::new().fg(theme::BLUE),
        CellKind::Out => theme::secondary(),
    }
}

fn path_spans(c: &PaCell, w: usize) -> Vec<Span<'static>> {
    let (p, ps) = path_text(c);
    let rbi = "•".repeat(c.rbi.min(4) as usize);
    let text_w = unicode_width::UnicodeWidthStr::width(p.as_str())
        + if rbi.is_empty() {
            0
        } else {
            1 + rbi.chars().count()
        };
    let left = w.saturating_sub(text_w) / 2;
    let mut out = vec![Span::raw(" ".repeat(left)), Span::styled(p, ps)];
    if !rbi.is_empty() {
        out.push(Span::raw(" "));
        out.push(Span::styled(rbi, Style::new().fg(theme::ORANGE)));
    }
    out.push(Span::raw(" ".repeat(w.saturating_sub(left + text_w))));
    out
}

fn path_text(c: &PaCell) -> (String, Style) {
    if c.reached == 4 {
        return ("◆".into(), theme::accent_bold());
    }
    if let Some(n) = c.out_number {
        let s = match n {
            1 => "①",
            2 => "②",
            _ => "③",
        };
        return (s.into(), theme::secondary());
    }
    if let Some(b) = c.out_at {
        return (
            format!("✕{}", if b == 4 { "H".into() } else { b.to_string() }),
            theme::error(),
        );
    }
    if c.reached > 0 {
        let dots: String = (1..=3)
            .map(|b| if b <= c.reached { '●' } else { '○' })
            .collect();
        return (dots, theme::accent());
    }
    (String::new(), theme::meta())
}

fn fit(s: &str, w: usize) -> String {
    if unicode_width::UnicodeWidthStr::width(s) > w {
        pad(s, w)
    } else {
        cpad(s, w)
    }
}

/// The scorer's backwards K for a called third strike (`KL` in the data).
pub const BACKWARDS_K: &str = "ꓘ";

/// Notation as shown: `KL` becomes the backwards K when that's on.
fn shown(notation: &str, backwards_k: bool) -> String {
    match notation.strip_prefix("KL") {
        Some(rest) if backwards_k => format!("{BACKWARDS_K}{rest}"),
        _ => notation.to_string(),
    }
}

fn cell_spans(cells: &[PaCell], backwards_k: bool) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let w = CELL_W - 1;
    match cells {
        [] => (
            vec![Span::raw(" ".repeat(w))],
            vec![Span::raw(" ".repeat(w))],
        ),
        [c] => (
            vec![Span::styled(
                fit(&shown(&c.notation, backwards_k), w),
                kind_style(c.kind),
            )],
            path_spans(c, w),
        ),
        // Batted twice in the inning: both results on the top line.
        many => {
            let top = many
                .iter()
                .map(|c| shown(&c.notation, backwards_k))
                .collect::<Vec<_>>()
                .join("/");
            (
                vec![Span::styled(fit(&top, w), kind_style(many[0].kind))],
                path_spans(many.last().unwrap(), w),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backwards_k_only_when_on() {
        assert_eq!(shown("KL", true), "ꓘ");
        assert_eq!(shown("KL DP", true), "ꓘ DP");
        assert_eq!(shown("KL", false), "KL");
        assert_eq!(shown("K", true), "K");
        assert_eq!(unicode_width::UnicodeWidthStr::width(BACKWARDS_K), 1);
    }
}
