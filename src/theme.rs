// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Ballatui contributors

//! Gruvbox dark palette with green as the primary accent and aqua as the
//! secondary one. Yellow is reserved for warnings.

use ratatui::style::{Color, Modifier, Style};

pub const BG0_H: Color = Color::Rgb(0x1d, 0x20, 0x21);
pub const BG0: Color = Color::Rgb(0x28, 0x28, 0x28);
pub const BG1: Color = Color::Rgb(0x3c, 0x38, 0x36);
pub const BG2: Color = Color::Rgb(0x50, 0x49, 0x45);
pub const BG3: Color = Color::Rgb(0x66, 0x5c, 0x54);
pub const BG4: Color = Color::Rgb(0x7c, 0x6f, 0x64);
pub const FG0: Color = Color::Rgb(0xfb, 0xf1, 0xc7);
pub const FG1: Color = Color::Rgb(0xeb, 0xdb, 0xb2);
pub const FG3: Color = Color::Rgb(0xbd, 0xae, 0x93);
pub const FG4: Color = Color::Rgb(0xa8, 0x99, 0x84);
pub const GRAY: Color = Color::Rgb(0x92, 0x83, 0x74);
pub const RED: Color = Color::Rgb(0xfb, 0x49, 0x34);
pub const GREEN: Color = Color::Rgb(0xb8, 0xbb, 0x26);
pub const YELLOW: Color = Color::Rgb(0xfa, 0xbd, 0x2f);
pub const BLUE: Color = Color::Rgb(0x83, 0xa5, 0x98);
pub const AQUA: Color = Color::Rgb(0x8e, 0xc0, 0x7c);
pub const ORANGE: Color = Color::Rgb(0xfe, 0x80, 0x19);

pub const ACCENT: Color = GREEN;
pub const ACCENT_2: Color = AQUA;

pub fn text() -> Style {
    Style::new().fg(FG1)
}
pub fn title() -> Style {
    Style::new().fg(FG0).add_modifier(Modifier::BOLD)
}
pub fn meta() -> Style {
    Style::new().fg(GRAY)
}
pub fn secondary() -> Style {
    Style::new().fg(FG3)
}
pub fn accent() -> Style {
    Style::new().fg(ACCENT)
}
pub fn accent_bold() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub fn accent2() -> Style {
    Style::new().fg(ACCENT_2)
}
pub fn key() -> Style {
    Style::new().fg(BLUE)
}
pub fn selected() -> Style {
    Style::new().bg(BG1)
}
pub fn border(focused: bool) -> Style {
    if focused {
        Style::new().fg(ACCENT)
    } else {
        Style::new().fg(BG4)
    }
}
pub fn warn() -> Style {
    Style::new().fg(YELLOW)
}
pub fn error() -> Style {
    Style::new().fg(RED)
}

/// Pitch result colors, matching the usual gamecast convention:
/// balls, strikes and balls in play each get their own color.
pub fn pitch_color(code: &str) -> Color {
    match code {
        "B" | "*B" | "V" | "P" | "I" | "H" => BLUE,
        "X" | "D" | "E" => GREEN,
        "F" | "L" | "R" => ORANGE,
        _ => RED, // called / swinging strikes, foul tips, missed bunts
    }
}
