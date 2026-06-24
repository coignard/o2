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

fn evolve(grid: &str) -> OxygenEngine {
    evolve_at(grid, 0)
}

fn evolve_at(grid: &str, frame: usize) -> OxygenEngine {
    let mut engine = OxygenEngine::new(1, 1, 1);
    engine.load_grid(grid);
    engine.f = frame;
    let mut midi = MidiEngine::new();
    engine.tick(&mut midi);
    engine
}

fn at(engine: &OxygenEngine, x: usize, y: usize) -> char {
    engine.cells[y * engine.w + x]
}

#[test]
fn add_writes_sum_below() {
    assert_eq!(at(&evolve("1A2\n..."), 1, 1), '3');
}

#[test]
fn add_wraps_in_base36() {
    assert_eq!(at(&evolve("zAz\n..."), 1, 1), 'y');
}

#[test]
fn add_uppercases_output_when_right_operand_is_uppercase() {
    assert_eq!(at(&evolve("1AB\n..."), 1, 1), 'C');
}

#[test]
fn subtract_writes_absolute_difference() {
    assert_eq!(at(&evolve("5B2\n..."), 1, 1), '3');
}

#[test]
fn multiply_writes_product() {
    assert_eq!(at(&evolve("3M2\n..."), 1, 1), '6');
}

#[test]
fn lesser_writes_minimum() {
    assert_eq!(at(&evolve("5L3\n..."), 1, 1), '3');
}

#[test]
fn clock_counts_modulo_on_rate() {
    assert_eq!(at(&evolve_at("2C4\n...", 4), 1, 1), '2');
}

#[test]
fn clock_is_silent_when_modulo_is_zero() {
    assert_eq!(at(&evolve_at("2C0\n...", 4), 1, 1), '.');
}

#[test]
fn delay_bangs_on_interval_boundary() {
    assert_eq!(at(&evolve_at("2D3\n...", 6), 1, 1), '*');
}

#[test]
fn delay_is_silent_between_boundaries() {
    assert_eq!(at(&evolve_at("2D3\n...", 1), 1, 1), '.');
}

#[test]
fn delay_bangs_every_frame_when_modulo_is_one() {
    assert_eq!(at(&evolve_at("1D1\n...", 3), 1, 1), '*');
}

#[test]
fn if_bangs_when_operands_match() {
    assert_eq!(at(&evolve("1F1\n..."), 1, 1), '*');
}

#[test]
fn if_is_silent_when_operands_differ() {
    assert_eq!(at(&evolve("1F2\n..."), 1, 1), '.');
}

#[test]
fn increment_steps_modulo() {
    assert_eq!(at(&evolve("1I4\n..."), 1, 1), '1');
}

#[test]
fn increment_writes_zero_when_modulo_is_zero() {
    assert_eq!(at(&evolve("1I0\n..."), 1, 1), '0');
}

#[test]
fn generator_copies_input_to_target() {
    assert_eq!(at(&evolve("001G5\n....."), 3, 1), '5');
}

#[test]
fn halt_locks_southward_operator() {
    assert_eq!(at(&evolve(".H..\n1I4.\n...."), 1, 2), '.');
}

#[test]
fn unhalted_operator_runs() {
    assert_eq!(at(&evolve("....\n1I4.\n...."), 1, 2), '1');
}

#[test]
fn jumper_copies_value_southward() {
    assert_eq!(at(&evolve("5..\nJ..\n..."), 0, 2), '5');
}

#[test]
fn jymper_copies_value_eastward() {
    assert_eq!(at(&evolve("5Y."), 2, 0), '5');
}

#[test]
fn konkat_reads_variable_written_same_frame() {
    assert_eq!(at(&evolve("aV5.Ka\n......"), 5, 1), '5');
}

#[test]
fn read_fetches_distant_cell() {
    assert_eq!(at(&evolve("10O.7\n....."), 2, 1), '7');
}

#[test]
fn push_writes_value_at_indexed_column() {
    assert_eq!(at(&evolve("12P5\n...."), 3, 1), '5');
}

#[test]
fn query_copies_window_below() {
    assert_eq!(at(&evolve("001Q5\n....."), 3, 1), '5');
}

#[test]
fn track_selects_first_lane() {
    assert_eq!(at(&evolve("02Tab\n....."), 2, 1), 'a');
}

#[test]
fn track_selects_second_lane() {
    assert_eq!(at(&evolve("12Tab\n....."), 2, 1), 'b');
}

#[test]
fn uclid_bangs_on_pulse() {
    assert_eq!(at(&evolve_at("4U8\n...", 0), 1, 1), '*');
}

#[test]
fn uclid_is_silent_off_pulse() {
    assert_eq!(at(&evolve_at("4U8\n...", 1), 1, 1), '.');
}

#[test]
fn variable_read_returns_written_value() {
    assert_eq!(at(&evolve("aV5\n.Va\n..."), 1, 2), '5');
}

#[test]
fn write_places_value_at_offset() {
    assert_eq!(at(&evolve("12X5\n....\n....\n...."), 3, 3), '5');
}

#[test]
fn lerp_steps_toward_target() {
    assert_eq!(at(&evolve("1Z8\n..."), 1, 1), '1');
}

#[test]
fn east_moves_into_empty_cell() {
    let e = evolve("E.");
    assert_eq!(at(&e, 1, 0), 'E');
    assert_eq!(at(&e, 0, 0), '.');
}

#[test]
fn east_bangs_when_blocked_by_edge() {
    assert_eq!(at(&evolve("..E"), 2, 0), '*');
}

#[test]
fn bang_erases_itself() {
    assert_eq!(at(&evolve("*.."), 0, 0), '.');
}

#[test]
fn comment_locks_operators_in_span() {
    assert_eq!(at(&evolve("#1A2#\n....."), 2, 1), '.');
}

#[test]
fn operator_outside_comment_runs() {
    assert_eq!(at(&evolve(".1A2.\n....."), 2, 1), '3');
}

#[test]
fn lowercase_operator_is_dormant_without_bang() {
    assert_eq!(at(&evolve("1a2\n..."), 1, 1), '.');
}

#[test]
fn lowercase_operator_runs_when_banged() {
    assert_eq!(at(&evolve("1a*\n..."), 1, 1), '1');
}
