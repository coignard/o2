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

use o2_rs::core::glyph::{is_allowed, is_operator, key_of, value_of};

#[test]
fn value_of_reads_base36_digits() {
    assert_eq!(value_of('0'), 0);
    assert_eq!(value_of('9'), 9);
    assert_eq!(value_of('a'), 10);
    assert_eq!(value_of('z'), 35);
}

#[test]
fn value_of_is_case_insensitive() {
    assert_eq!(value_of('A'), 10);
    assert_eq!(value_of('Z'), 35);
}

#[test]
fn value_of_treats_dot_and_bang_as_zero() {
    assert_eq!(value_of('.'), 0);
    assert_eq!(value_of('*'), 0);
}

#[test]
fn key_of_is_inverse_of_value_of() {
    for v in 0..36 {
        assert_eq!(value_of(key_of(v, false)), v);
    }
}

#[test]
fn key_of_wraps_modulo_base36() {
    assert_eq!(key_of(36, false), '0');
    assert_eq!(key_of(37, false), '1');
}

#[test]
fn key_of_uppercases_on_request() {
    assert_eq!(key_of(10, true), 'A');
    assert_eq!(key_of(10, false), 'a');
}

#[test]
fn underscore_is_allowed_but_not_an_operator() {
    assert!(is_allowed('_'));
    assert!(!is_operator('_'));
}

#[test]
fn allowed_accepts_glyphs_and_specials() {
    for g in ['.', 'a', 'Z', '5', '*', '#', '$', ';'] {
        assert!(is_allowed(g), "{g}");
    }
}

#[test]
fn allowed_rejects_foreign_characters() {
    for g in [' ', '@', '/', '\n'] {
        assert!(!is_allowed(g), "{g:?}");
    }
}

#[test]
fn operators_are_letters_and_special_symbols() {
    for g in ['a', 'A', 'z', '*', '#', ':', '%', '!', '?', '=', ';', '$'] {
        assert!(is_operator(g), "{g}");
    }
}

#[test]
fn digits_and_dot_are_not_operators() {
    for g in ['0', '5', '9', '.'] {
        assert!(!is_operator(g), "{g}");
    }
}
