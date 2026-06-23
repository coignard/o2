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

pub const BASE36_RADIX: usize = 36;
pub const ALLOWED_SPECIAL_CHARS: &str = "*#$!%:?=;_";
pub const OPERATOR_SPECIAL_CHARS: &str = "*#$!%:?=;";

pub fn is_allowed(g: char) -> bool {
    let gl = g.to_ascii_lowercase();
    gl == '.' || gl.is_ascii_alphanumeric() || ALLOWED_SPECIAL_CHARS.contains(gl)
}

pub fn is_operator(g: char) -> bool {
    let gl = g.to_ascii_lowercase();
    gl.is_ascii_alphabetic() || OPERATOR_SPECIAL_CHARS.contains(gl)
}

pub fn value_of(g: char) -> usize {
    g.to_digit(36).unwrap_or(0) as usize
}

pub fn key_of(val: usize, uppercase: bool) -> char {
    let c = std::char::from_digit((val % BASE36_RADIX) as u32, BASE36_RADIX as u32).unwrap_or('0');
    if uppercase { c.to_ascii_uppercase() } else { c }
}

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
