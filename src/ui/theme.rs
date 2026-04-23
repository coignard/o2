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

//! Colour palette and style-type definitions.
//!
//! The nine base colours are declared as constants and a [`StyleType`] enum
//! maps each semantic rendering category to a foreground/background pair.
//! Both the constants and the enum correspond to the nine-slot theme format
//! used throughout the application.

#![allow(dead_code)]

use ratatui::style::Color;

/// Pure white; used for high-contrast foreground elements.
pub const F_HIGH: Color = Color::Rgb(255, 255, 255);

/// Medium grey; used for secondary foreground text (status bar labels, etc.).
pub const F_MED: Color = Color::Rgb(119, 119, 119);

/// Dark grey; the default foreground colour for most grid glyphs.
pub const F_LOW: Color = Color::Rgb(68, 68, 68);

/// Black; foreground on inverted or selected cells.
pub const F_INV: Color = Color::Rgb(0, 0, 0);

/// Near-white; the primary output and highlighted text colour.
pub const B_HIGH: Color = Color::Rgb(238, 238, 238);

/// Teal / cyan; operator and haste port accent colour.
pub const B_MED: Color = Color::Rgb(114, 222, 194);

/// Dark grey; used sparingly for background accents.
pub const B_LOW: Color = Color::Rgb(68, 68, 68);

/// Amber / orange; selection highlight and reader accent colour.
pub const B_INV: Color = Color::Rgb(255, 181, 69);

/// True black; the canvas background colour.
pub const BG: Color = Color::Rgb(0, 0, 0);

/// Semantic rendering categories used by the grid and status-bar renderer.
///
/// Each variant corresponds to a distinct visual role. The [`StyleType::colors`]
/// method resolves a variant to its `(foreground, background)` colour pair,
/// where `None` means "inherit from context" (i.e. use the canvas background).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum StyleType {
    /// The operator glyph itself: teal background with dark foreground.
    Operator,
    /// Left-side or upward input ports (haste ports): teal foreground.
    Haste,
    /// Standard right-side or downward input ports: near-white foreground.
    Input,
    /// Output ports: near-white background with dark foreground.
    Output,
    /// Cells within the current cursor selection: amber background, black text.
    Selected,
    /// Cells locked by an operator this frame: grey foreground.
    Locked,
    /// Cells whose glyph matches the glyph under the cursor (reader
    /// highlighting): amber foreground.
    Reader,
    /// Clock status indicator: amber foreground.
    Clock,
    /// Ordinary glyphs with no special decoration: dark grey foreground.
    #[default]
    Default,
}

impl StyleType {
    /// Returns the `(foreground, background)` colour pair for this style type.
    ///
    /// A `None` value means the colour should not be explicitly set; the
    /// renderer falls back to the canvas background.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::ui::theme::StyleType;
    ///
    /// let (fg, bg) = StyleType::Selected.colors();
    /// assert!(fg.is_some());
    /// assert!(bg.is_some());
    ///
    /// let (_, bg_default) = StyleType::Default.colors();
    /// assert!(bg_default.is_none());
    ///
    /// let (fg_locked, _) = StyleType::Locked.colors();
    /// assert!(fg_locked.is_some());
    /// ```
    pub fn colors(self) -> (Option<Color>, Option<Color>) {
        match self {
            Self::Operator => (Some(F_LOW), Some(B_MED)),
            Self::Haste => (Some(B_MED), None),
            Self::Input => (Some(B_HIGH), None),
            Self::Output => (Some(F_LOW), Some(B_HIGH)),
            Self::Selected => (Some(F_INV), Some(B_INV)),
            Self::Locked => (Some(F_MED), None),
            Self::Reader => (Some(B_INV), None),
            Self::Clock => (Some(B_INV), None),
            Self::Default => (Some(F_LOW), None),
        }
    }
}

/// Scales the brightness of an RGB colour to the specified percentage.
///
/// The `percent` argument determines the remaining brightness of the colour:
/// a value of `100` leaves the colour completely unchanged, `50` reduces its
/// channel values by half, and `0` yields pure black. Values above `100` will
/// brighten the colour (clamping at 255 due to `u8` conversion limits).
///
/// Note that this function only operates on [`Color::Rgb`] variants. If any
/// other [`Color`] variant is passed (such as named or ANSI indexed colours),
/// it is returned unmodified.
///
/// # Examples
///
/// ```
/// use ratatui::style::Color;
/// use o2_rs::ui::theme::darken;
///
/// let base = Color::Rgb(100, 200, 50);
/// let dark = darken(base, 60); // 60% of original brightness
/// assert_eq!(dark, Color::Rgb(60, 120, 30));
/// ```
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
