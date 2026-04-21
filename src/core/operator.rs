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

//! Operator metadata: human-readable names for each operator glyph.

/// Returns the human-readable name for an operator glyph.
///
/// The glyph should be passed in **lowercase** (or as a special symbol);
/// uppercase variants of letter-based operators share the same name as their
/// lowercase counterpart.
///
/// The returned string is used by the UI inspector to label port cells and
/// describe the operator under the cursor.
///
/// # Examples
///
/// ```
/// use o2_rs::core::operator::operator_name;
///
/// assert_eq!(operator_name('a'), "add");
/// assert_eq!(operator_name('m'), "multiply");
/// assert_eq!(operator_name(':'), "midi");
/// assert_eq!(operator_name('$'), "self");
/// assert_eq!(operator_name('?'), "pb");
/// assert_eq!(operator_name('~'), "unknown");
/// ```
pub fn operator_name(gl: char) -> &'static str {
    match gl {
        'a' => "add",
        'b' => "subtract",
        'c' => "clock",
        'd' => "delay",
        'e' => "east",
        'f' => "if",
        'g' => "generator",
        'h' => "halt",
        'i' => "increment",
        'j' => "jumper",
        'k' => "konkat",
        'l' => "lesser",
        'm' => "multiply",
        'n' => "north",
        'o' => "read",
        'p' => "push",
        'q' => "query",
        'r' => "random",
        's' => "south",
        't' => "track",
        'u' => "uclid",
        'v' => "variable",
        'w' => "west",
        'x' => "write",
        'y' => "jymper",
        'z' => "lerp",
        '*' => "bang",
        '#' => "comment",
        ':' => "midi",
        '%' => "mono",
        '!' => "cc",
        '?' => "pb",
        '=' => "osc",
        ';' => "udp",
        '$' => "self",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_operator_name_edge_cases() {
        assert_eq!(operator_name('\0'), "unknown");
        assert_eq!(operator_name(' '), "unknown");
        assert_eq!(operator_name('A'), "unknown");
        assert_eq!(operator_name('5'), "unknown");
        assert_eq!(operator_name('ё'), "unknown");
        assert_eq!(operator_name('🍁'), "unknown");
    }

    proptest! {
        #[test]
        fn prop_operator_name_never_panics(c in any::<char>()) {
            let _ = operator_name(c);
        }
    }
}
