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

//! Note transposition table: maps ORCΛ glyphs to MIDI note IDs.
//!
//! The mapping is implemented as a `match` expression for zero-allocation,
//! branch-table-friendly performance.
//!
//! # Note encoding
//!
//! Each glyph encodes a note letter (`C`, `D`, `E`, ... `B`) and an octave
//! offset. Lowercase letters indicate sharps (e.g. `c` = C#, `d` = D#).
//! The resulting MIDI ID is clamped to `[0, 127]`.

/// Converts an ORCΛ note glyph and a base octave to a MIDI note ID.
///
/// Returns `None` for glyphs that do not represent a note (digits, `'.'`, `'*'`,
/// etc.). The `base_octave` value is clamped so that the final MIDI ID stays
/// within `[24, 127]`.
///
/// # Examples
///
/// ```
/// use o2_rs::core::transpose::transpose;
///
/// assert_eq!(transpose('C', 3), Some(60)); // middle C
/// assert_eq!(transpose('c', 0), Some(25)); // C# in octave 0
/// assert_eq!(transpose('1', 4), None);     // not a note glyph
/// assert_eq!(transpose('.', 0), None);
/// ```
pub fn transpose(glyph: char, base_octave: i32) -> Option<u8> {
    let (note_letter, octave_offset) = match glyph {
        'A' => ('A', 0),
        'a' => ('a', 0),
        'B' => ('B', 0),
        'C' => ('C', 0),
        'c' => ('c', 0),
        'D' => ('D', 0),
        'd' => ('d', 0),
        'E' => ('E', 0),
        'F' => ('F', 0),
        'f' => ('f', 0),
        'G' => ('G', 0),
        'g' => ('g', 0),
        'H' => ('A', 0),
        'h' => ('a', 0),
        'I' => ('B', 0),
        'J' => ('C', 1),
        'j' => ('c', 1),
        'K' => ('D', 1),
        'k' => ('d', 1),
        'L' => ('E', 1),
        'M' => ('F', 1),
        'm' => ('f', 1),
        'N' => ('G', 1),
        'n' => ('g', 1),
        'O' => ('A', 1),
        'o' => ('a', 1),
        'P' => ('B', 1),
        'Q' => ('C', 2),
        'q' => ('c', 2),
        'R' => ('D', 2),
        'r' => ('d', 2),
        'S' => ('E', 2),
        'T' => ('F', 2),
        't' => ('f', 2),
        'U' => ('G', 2),
        'u' => ('g', 2),
        'V' => ('A', 2),
        'v' => ('a', 2),
        'W' => ('B', 2),
        'X' => ('C', 3),
        'x' => ('c', 3),
        'Y' => ('D', 3),
        'y' => ('d', 3),
        'Z' => ('E', 3),
        'e' => ('F', 0),
        'l' => ('F', 1),
        's' => ('F', 2),
        'z' => ('F', 3),
        'b' => ('C', 1),
        'i' => ('C', 1),
        'p' => ('C', 2),
        'w' => ('C', 3),
        _ => return None,
    };

    let octave = (base_octave + octave_offset).clamp(0, 8);

    let note_index = match note_letter {
        'C' => 0,
        'c' => 1,
        'D' => 2,
        'd' => 3,
        'E' => 4,
        'F' => 5,
        'f' => 6,
        'G' => 7,
        'g' => 8,
        'A' => 9,
        'a' => 10,
        'B' => 11,
        _ => 0,
    };

    let midi_id = ((octave * 12) + note_index + 24).clamp(0, 127);
    Some(midi_id as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transpose_major() {
        assert_eq!(transpose('C', 0), Some(24));
        assert_eq!(transpose('D', 0), Some(26));
        assert_eq!(transpose('E', 0), Some(28));
        assert_eq!(transpose('F', 0), Some(29));
        assert_eq!(transpose('G', 0), Some(31));
        assert_eq!(transpose('A', 0), Some(33));
        assert_eq!(transpose('B', 0), Some(35));
    }

    #[test]
    fn test_transpose_minor() {
        assert_eq!(transpose('c', 0), Some(25));
        assert_eq!(transpose('d', 0), Some(27));
        assert_eq!(transpose('f', 0), Some(30));
        assert_eq!(transpose('g', 0), Some(32));
        assert_eq!(transpose('a', 0), Some(34));
    }

    #[test]
    fn test_transpose_octaves() {
        assert_eq!(transpose('C', 3), Some(60));
        assert_eq!(transpose('C', 4), Some(72));
        assert_eq!(transpose('C', 8), Some(120));
        assert_eq!(transpose('C', -1), Some(24));
        assert_eq!(transpose('C', 9), Some(120));
        assert_eq!(transpose('A', 4), Some(81));
    }

    #[test]
    fn test_transpose_aliases() {
        assert_eq!(transpose('H', 0), Some(33));
        assert_eq!(transpose('h', 0), Some(34));
        assert_eq!(transpose('I', 0), Some(35));
        assert_eq!(transpose('J', 0), Some(36));
        assert_eq!(transpose('Z', 0), Some(64));
    }

    #[test]
    fn test_transpose_override() {
        assert_eq!(transpose('e', 0), Some(29));
        assert_eq!(transpose('l', 0), Some(41));
        assert_eq!(transpose('s', 0), Some(53));
        assert_eq!(transpose('z', 0), Some(65));
        assert_eq!(transpose('b', 0), Some(36));
        assert_eq!(transpose('i', 0), Some(36));
        assert_eq!(transpose('p', 0), Some(48));
        assert_eq!(transpose('w', 0), Some(60));
    }

    #[test]
    fn test_transpose_extreme_clamps() {
        assert_eq!(transpose('Z', 9), Some(124));
        assert_eq!(transpose('e', -5), Some(29));
        assert_eq!(transpose('c', -2), Some(25));
    }

    #[test]
    fn test_transpose_invalid() {
        assert_eq!(transpose('1', 0), None);
        assert_eq!(transpose('.', 0), None);
        assert_eq!(transpose('*', 0), None);
        assert_eq!(transpose('$', 0), None);
        assert_eq!(transpose('#', 0), None);
    }

    #[test]
    fn test_transpose_exhaustive() {
        let valid_chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        for c in valid_chars.chars() {
            let res = transpose(c, 4);
            assert!(res.is_some());
            let val = res.unwrap();
            assert!(val >= 24);
        }
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_transpose_bounds(c in any::<char>(), octave in -10..20i32) {
            if let Some(note) = transpose(c, octave) {
                assert!(note <= 127);
            }
        }

        #[test]
        fn prop_transpose_valid_chars(octave in 0..=8i32) {
            let valid_chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
            for c in valid_chars.chars() {
                let note = transpose(c, octave);
                assert!(note.is_some());
                let n = note.unwrap();
                assert!(n <= 127);
            }
        }
    }
}
