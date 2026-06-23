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
