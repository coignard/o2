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

pub const MIDI_NOTE_ON: u8 = 0x90;
pub const MIDI_NOTE_OFF: u8 = 0x80;
pub const MIDI_CC: u8 = 0xB0;
pub const MIDI_PITCH_BEND: u8 = 0xE0;
pub const MIDI_CHANNELS: usize = 16;
pub const DEFAULT_CC_OFFSET: u8 = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiNote {
    pub channel: u8,
    pub octave: u8,
    pub note: char,
    pub note_id: u8,
    pub velocity: u8,
    pub length: usize,
    pub is_played: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiCc {
    pub channel: u8,
    pub knob: u8,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiPb {
    pub channel: u8,
    pub lsb: u8,
    pub msb: u8,
}

#[derive(Debug, Clone)]
pub enum MidiMessage {
    Cc(MidiCc),
    Pb(MidiPb),
}

pub struct MidiEngine {
    pub stack: Vec<MidiNote>,
    pub mono_stack: [Option<MidiNote>; MIDI_CHANNELS],
    pub cc_stack: Vec<MidiMessage>,
    pub osc_buf: Vec<(String, String)>,
    pub udp_buf: Vec<String>,
    pub pending: Vec<Vec<u8>>,
    pub cc_offset: u8,
    pub last_io_count: usize,
}

impl MidiEngine {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            mono_stack: std::array::from_fn(|_| None),
            cc_stack: Vec::new(),
            osc_buf: Vec::new(),
            udp_buf: Vec::new(),
            pending: Vec::new(),
            cc_offset: DEFAULT_CC_OFFSET,
            last_io_count: 0,
        }
    }

    pub fn send_midi_msg(&mut self, msg: &[u8]) {
        self.pending.push(msg.to_vec());
    }

    pub fn count_io(&self) -> usize {
        self.stack.len()
            + self.mono_stack.iter().flatten().count()
            + self.cc_stack.len()
            + self.osc_buf.len()
            + self.udp_buf.len()
    }

    pub fn run(&mut self) {
        self.stack.retain_mut(|note| {
            if !note.is_played {
                self.pending.push(vec![
                    MIDI_NOTE_ON + note.channel,
                    note.note_id,
                    note.velocity,
                ]);
                note.is_played = true;
            }
            if note.length < 1 {
                self.pending
                    .push(vec![MIDI_NOTE_OFF + note.channel, note.note_id, 0]);
                false
            } else {
                note.length = note.length.saturating_sub(1);
                true
            }
        });

        for slot in self.mono_stack.iter_mut() {
            if let Some(note) = slot {
                if note.length < 1 {
                    if note.is_played {
                        self.pending
                            .push(vec![MIDI_NOTE_OFF + note.channel, note.note_id, 0]);
                    }
                    *slot = None;
                    continue;
                }
                if !note.is_played {
                    self.pending.push(vec![
                        MIDI_NOTE_ON + note.channel,
                        note.note_id,
                        note.velocity,
                    ]);
                    note.is_played = true;
                }
                note.length = note.length.saturating_sub(1);
            }
        }

        for msg in self.cc_stack.drain(..) {
            match msg {
                MidiMessage::Cc(cc) => {
                    let knob_val = self.cc_offset.saturating_add(cc.knob).min(127);
                    self.pending
                        .push(vec![MIDI_CC + cc.channel, knob_val, cc.value]);
                }
                MidiMessage::Pb(pb) => {
                    self.pending
                        .push(vec![MIDI_PITCH_BEND + pb.channel, pb.lsb, pb.msb]);
                }
            }
        }
    }

    pub fn silence(&mut self) {
        self.stack.clear();
        self.mono_stack = std::array::from_fn(|_| None);
        self.cc_stack.clear();
        self.osc_buf.clear();
        self.udp_buf.clear();
        self.pending.clear();
    }
}

impl Default for MidiEngine {
    fn default() -> Self {
        Self::new()
    }
}
