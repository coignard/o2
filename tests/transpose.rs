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

use o2_rs::core::transpose::transpose;

#[test]
fn transpose_maps_c_to_middle_c() {
    assert_eq!(transpose('C', 3), Some(60));
}

#[test]
fn transpose_maps_a_to_concert_pitch() {
    assert_eq!(transpose('A', 3), Some(69));
}

#[test]
fn transpose_applies_octave_offset() {
    assert_eq!(transpose('J', 3), Some(72));
}

#[test]
fn transpose_clamps_octave_to_lower_bound() {
    assert_eq!(transpose('C', -5), Some(24));
}

#[test]
fn transpose_clamps_octave_to_upper_bound() {
    assert_eq!(transpose('C', 20), Some(120));
}

#[test]
fn transpose_maps_lowercase_specials_to_sharps() {
    assert_eq!(transpose('e', 0), Some(29));
    assert_eq!(transpose('z', 3), Some(101));
}

#[test]
fn transpose_rejects_non_note_glyphs() {
    assert_eq!(transpose('.', 3), None);
    assert_eq!(transpose('1', 3), None);
    assert_eq!(transpose('+', 3), None);
}
