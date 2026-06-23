// This file is part of o2.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use o2_rs::core::oxygen::StyleType;
use ratatui::style::Color;

pub const F_HIGH: Color = Color::Rgb(255, 255, 255);
pub const F_MED: Color = Color::Rgb(119, 119, 119);
pub const F_LOW: Color = Color::Rgb(68, 68, 68);
pub const F_INV: Color = Color::Rgb(0, 0, 0);
pub const B_HIGH: Color = Color::Rgb(238, 238, 238);
pub const B_MED: Color = Color::Rgb(114, 222, 194);
pub const B_INV: Color = Color::Rgb(255, 181, 69);
pub const BG: Color = Color::Rgb(0, 0, 0);

#[allow(dead_code)]
pub const B_LOW: Color = Color::Rgb(68, 68, 68);

pub const fn darken(color: Color, percent: u16) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            ((r as u16 * percent) / 100) as u8,
            ((g as u16 * percent) / 100) as u8,
            ((b as u16 * percent) / 100) as u8,
        ),
        _ => color,
    }
}

pub fn style_colors(s: StyleType) -> (Option<Color>, Option<Color>) {
    match s {
        StyleType::Operator => (Some(F_LOW), Some(B_MED)),
        StyleType::Haste => (Some(B_MED), None),
        StyleType::Input => (Some(B_HIGH), None),
        StyleType::Output => (Some(F_LOW), Some(B_HIGH)),
        StyleType::Selected => (Some(F_INV), Some(B_INV)),
        StyleType::Locked => (Some(F_MED), None),
        StyleType::Reader => (Some(B_INV), None),
        StyleType::Clock => (Some(B_INV), None),
        StyleType::Default => (Some(F_LOW), None),
    }
}
