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

use o2_rs::core::midi::{MIDI_CC, MIDI_NOTE_OFF, MIDI_NOTE_ON, MIDI_PITCH_BEND, MidiEngine};
use o2_rs::core::oxygen::OxygenEngine;

fn play(grid: &str) -> (OxygenEngine, MidiEngine) {
    let mut engine = OxygenEngine::new(1, 1, 1);
    engine.load_grid(grid);
    let mut midi = MidiEngine::new();
    engine.tick(&mut midi);
    (engine, midi)
}

#[test]
fn note_emits_note_on_with_transposed_pitch() {
    let (_e, mut m) = play(":01c..\n*.....");
    m.run();
    assert_eq!(m.pending, vec![vec![MIDI_NOTE_ON, 37, 119]]);
}

#[test]
fn note_releases_after_its_length() {
    let (_e, mut m) = play(":01c..\n*.....");
    m.run();
    m.pending.clear();
    m.run();
    assert_eq!(m.pending, vec![vec![MIDI_NOTE_OFF, 37, 0]]);
}

#[test]
fn velocity_scales_glyph_to_seven_bit() {
    let (_e, mut m) = play(":01c8.\n*.....");
    m.run();
    assert_eq!(m.pending[0][2], 63);
}

#[test]
fn velocity_saturates_at_maximum() {
    let (_e, mut m) = play(":01cg.\n*.....");
    m.run();
    assert_eq!(m.pending[0][2], 127);
}

#[test]
fn channel_selects_status_byte() {
    let (_e, mut m) = play(":51c..\n*.....");
    m.run();
    assert_eq!(m.pending[0][0], MIDI_NOTE_ON + 5);
}

#[test]
fn poly_note_lands_on_the_stack() {
    let (_e, m) = play(":01c..\n*.....");
    assert_eq!(m.stack.len(), 1);
    assert!(m.mono_stack.iter().all(|s| s.is_none()));
}

#[test]
fn mono_note_lands_on_the_channel_slot() {
    let (_e, m) = play("%01c..\n*.....");
    assert!(m.stack.is_empty());
    assert!(m.mono_stack[0].is_some());
}

#[test]
fn tied_note_is_held_without_retrigger() {
    let (_e, mut m) = play(":01c._\n*.....");
    m.run();
    m.pending.clear();
    m.run();
    assert!(m.pending.is_empty());
    assert_eq!(m.stack.len(), 1);
}

#[test]
fn control_change_scales_value_and_offsets_knob() {
    let (_e, mut m) = play("!0az.\n*....");
    m.run();
    assert_eq!(m.pending, vec![vec![MIDI_CC, 64 + 10, 127]]);
}

#[test]
fn pitch_bend_scales_both_bytes() {
    let (_e, mut m) = play("?0zz.\n*....");
    m.run();
    assert_eq!(m.pending, vec![vec![MIDI_PITCH_BEND, 127, 127]]);
}
