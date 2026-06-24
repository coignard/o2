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

use o2_rs::core::midi::MidiEngine;
use o2_rs::core::oxygen::OxygenEngine;

fn loaded(grid: &str) -> OxygenEngine {
    let mut engine = OxygenEngine::new(1, 1, 1);
    engine.load_grid(grid);
    engine
}

#[test]
fn load_grid_derives_dimensions_from_content() {
    let e = loaded("abc\nde");
    assert_eq!((e.w, e.h), (3, 2));
}

#[test]
fn load_grid_pads_short_lines_with_dots() {
    let e = loaded("abc\nde");
    assert_eq!(e.cells[e.w + 2], '.');
}

#[test]
fn load_grid_keeps_underscore_glyph() {
    let e = loaded("a_b");
    assert_eq!(e.cells[1], '_');
}

#[test]
fn load_grid_rejects_disallowed_characters() {
    let e = loaded("a b");
    assert_eq!(e.cells[1], '.');
}

#[test]
fn resize_preserves_existing_cells() {
    let mut e = loaded("AB\nCD");
    e.resize_grid(4, 4);
    assert_eq!((e.w, e.h), (4, 4));
    assert_eq!(e.cells[0], 'A');
    assert_eq!(e.cells[e.w + 1], 'D');
    assert_eq!(e.cells[e.cells.len() - 1], '.');
}

#[test]
fn resize_clamps_to_minimum_dimensions() {
    let mut e = loaded("AB\nCD");
    e.resize_grid(0, 0);
    assert_eq!((e.w, e.h), (1, 1));
}

#[test]
fn resize_keeps_parallel_buffers_consistent() {
    let mut e = loaded("AB\nCD");
    e.resize_grid(6, 5);
    let area = e.w * e.h;
    assert_eq!(e.cells.len(), area);
    assert_eq!(e.locks.len(), area);
    assert_eq!(e.ports.len(), area);
    assert_eq!(e.port_names.len(), area);
}

#[test]
fn random_is_pure_for_fixed_inputs() {
    let mut e = OxygenEngine::new(8, 8, 99);
    e.f = 5;
    assert_eq!(e.random(2, 2, 0, 35), e.random(2, 2, 0, 35));
}

#[test]
fn random_is_deterministic_for_equal_seed() {
    let a = OxygenEngine::new(8, 8, 42);
    let b = OxygenEngine::new(8, 8, 42);
    assert_eq!(a.random(3, 4, 0, 35), b.random(3, 4, 0, 35));
}

#[test]
fn random_stays_within_inclusive_range() {
    let mut e = OxygenEngine::new(8, 8, 7);
    for f in 0..256 {
        e.f = f;
        let v = e.random(2, 3, 3, 9);
        assert!((3..=9).contains(&v), "frame {f} produced {v}");
    }
}

#[test]
fn random_collapses_when_bounds_are_equal() {
    let e = OxygenEngine::new(8, 8, 7);
    assert_eq!(e.random(0, 0, 5, 5), 5);
}

#[test]
fn random_is_order_independent_for_bounds() {
    let e = OxygenEngine::new(8, 8, 7);
    assert_eq!(e.random(1, 1, 2, 9), e.random(1, 1, 9, 2));
}

#[test]
fn self_defers_command_to_end_of_frame() {
    let mut e = loaded("$ab.\n*...");
    let mut m = MidiEngine::new();
    e.tick(&mut m);
    assert_eq!(e.commands, vec![("ab".to_string(), Some((0, 1)))]);
}

#[test]
fn tick_clears_variables_from_previous_frame() {
    let mut e = loaded("aV5\n...");
    let mut m = MidiEngine::new();
    e.tick(&mut m);
    e.load_grid("...\n...");
    e.tick(&mut m);
    assert!(e.variables.iter().all(|v| v.is_none()));
}
