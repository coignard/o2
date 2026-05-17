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

//! Glyph classification, base-36 codec, and human-readable operator names.
//!
//! This module is the single source of truth for deciding which characters are
//! valid grid content, which characters are operators, and how to convert
//! between numeric values and their base-36 glyph representations.

/// Base-36 radix used for glyph ↔ numeric value conversion.
pub const BASE36_RADIX: usize = 36;

/// Characters permitted in the grid beyond alphanumerics and `'.'`.
pub const ALLOWED_SPECIAL_CHARS: &str = "*#$!%:?=;_";

/// Characters that are treated as operator glyphs (subset of [`ALLOWED_SPECIAL_CHARS`],
/// excluding the underscore which is a data glyph, not an operator).
pub const OPERATOR_SPECIAL_CHARS: &str = "*#$!%:?=;";

/// Returns `true` if the character `g` is permitted in the grid.
///
/// # Examples
///
/// ```
/// use o2_rs::core::glyph::is_allowed;
///
/// assert!(is_allowed('.'));
/// assert!(is_allowed('A'));
/// assert!(is_allowed(':'));
/// assert!(!is_allowed(' '));
/// assert!(!is_allowed('-'));
/// ```
pub fn is_allowed(g: char) -> bool {
    let gl = g.to_ascii_lowercase();
    gl == '.' || gl.is_ascii_alphanumeric() || ALLOWED_SPECIAL_CHARS.contains(gl)
}

/// Returns `true` if `g` is a recognised operator glyph.
///
/// # Examples
///
/// ```
/// use o2_rs::core::glyph::is_operator;
///
/// assert!(is_operator('A'));
/// assert!(is_operator('*'));
/// assert!(!is_operator('5'));
/// ```
pub fn is_operator(g: char) -> bool {
    let gl = g.to_ascii_lowercase();
    gl.is_ascii_alphabetic() || OPERATOR_SPECIAL_CHARS.contains(gl)
}

/// Converts a base-36 glyph to its numeric value.
///
/// # Examples
///
/// ```
/// use o2_rs::core::glyph::value_of;
///
/// assert_eq!(value_of('0'), 0);
/// assert_eq!(value_of('9'), 9);
/// assert_eq!(value_of('a'), 10);
/// assert_eq!(value_of('z'), 35);
/// assert_eq!(value_of('.'), 0);
/// ```
pub fn value_of(g: char) -> usize {
    g.to_digit(36).unwrap_or(0) as usize
}

/// Converts a numeric value to its base-36 glyph representation.
///
/// # Examples
///
/// ```
/// use o2_rs::core::glyph::key_of;
///
/// assert_eq!(key_of(0, false), '0');
/// assert_eq!(key_of(10, false), 'a');
/// assert_eq!(key_of(10, true), 'A');
/// assert_eq!(key_of(36, false), '0');
/// ```
pub fn key_of(val: usize, uppercase: bool) -> char {
    let c = std::char::from_digit((val % BASE36_RADIX) as u32, BASE36_RADIX as u32).unwrap_or('0');
    if uppercase { c.to_ascii_uppercase() } else { c }
}

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
/// use o2_rs::core::glyph::operator_name;
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

    #[test]
    fn test_is_allowed() {
        assert!(is_allowed('.'));
        assert!(is_allowed('A'));
        assert!(is_allowed('a'));
        assert!(is_allowed('0'));
        assert!(is_allowed('*'));
        assert!(is_allowed('#'));
        assert!(is_allowed('_'));
        assert!(!is_allowed(' '));
        assert!(!is_allowed('-'));
        assert!(!is_allowed('\t'));
    }

    #[test]
    fn test_is_operator() {
        assert!(is_operator('A'));
        assert!(is_operator('a'));
        assert!(is_operator('*'));
        assert!(is_operator('#'));
        assert!(!is_operator('.'));
        assert!(!is_operator('5'));
        assert!(!is_operator('_'));
    }

    #[test]
    fn test_base36_roundtrip() {
        for val in 0..36usize {
            let g = key_of(val, false);
            assert_eq!(value_of(g), val);
        }
    }

    proptest! {
        #[test]
        fn prop_operator_name_never_panics(c in any::<char>()) {
            let _ = operator_name(c);
        }

        #[test]
        fn prop_is_allowed_never_panics(c in any::<char>()) {
            let _ = is_allowed(c);
        }

        #[test]
        fn prop_value_of_and_key_of_roundtrip(val in any::<usize>()) {
            let ch_lower = key_of(val, false);
            assert_eq!(value_of(ch_lower), val % 36);
            let ch_upper = key_of(val, true);
            assert_eq!(value_of(ch_upper), val % 36);
        }
    }
}
