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

//! MIDI output, note stacks, CC/PB messages, OSC, and UDP.

use crate::core::io::osc::Osc;
use crate::core::io::udp::Udp;
use midir::{MidiInput, MidiOutput, MidiOutputConnection};
use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::net::UdpSocket;

/// A single note event in the polyphonic playback stack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiNote {
    /// MIDI channel (0..15).
    pub channel: u8,
    /// Base octave used when calculating the note ID.
    pub octave: u8,
    /// The ORCΛ note glyph that was used to create this note.
    pub note: char,
    /// Resolved MIDI note number (0..127).
    pub note_id: u8,
    /// MIDI velocity (0..127).
    pub velocity: u8,
    /// Remaining frame count before Note Off is sent.
    pub length: usize,
    /// `true` once the Note On message has been transmitted.
    pub is_played: bool,
}

/// A MIDI Control Change message, queued by the `!` operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiCc {
    /// MIDI channel (0..15).
    pub channel: u8,
    /// Controller number (added to the global CC offset before sending).
    pub knob: u8,
    /// Controller value (0..127).
    pub value: u8,
}

/// A MIDI Pitch Bend message, queued by the `?` operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiPb {
    /// MIDI channel (0..15).
    pub channel: u8,
    /// LSB of the 14-bit pitch bend value.
    pub lsb: u8,
    /// MSB of the 14-bit pitch bend value.
    pub msb: u8,
}

/// A tagged union covering both CC and Pitch Bend outgoing messages.
#[derive(Debug)]
pub enum MidiMessage {
    /// A Control Change message.
    Cc(MidiCc),
    /// A Pitch Bend message.
    Pb(MidiPb),
}

/// All MIDI, OSC, and UDP runtime state owned by the application.
pub struct MidiState {
    /// Active connection to the selected MIDI output device.
    pub out: Option<MidiOutputConnection>,
    /// Polyphonic note stack: notes are added by `:` and removed after their
    /// `length` counts down to zero.
    pub stack: Vec<MidiNote>,
    /// Monophonic per-channel slots populated by `%`. Each channel can hold at
    /// most one active note at a time.
    pub mono_stack: [Option<MidiNote>; 16],
    /// Pending CC and Pitch Bend messages, cleared after each [`run`](MidiState::run) call.
    pub cc_stack: Vec<MidiMessage>,
    /// OSC output state: pending message queue and destination port.
    pub osc: Osc,
    /// UDP output state: pending datagram queue and destination port.
    pub udp: Udp,
    /// Base controller number added to every CC knob value before transmitting.
    pub cc_offset: u8,
    /// Display name of the currently selected MIDI output device.
    pub device_name: String,
    /// Display name of the currently selected MIDI input device.
    pub input_device_name: String,
    /// Index of the selected output device in the device list, or `-1` for none.
    pub output_index: i32,
    /// Index of the selected input device in the device list, or `-1` for none.
    pub input_index: i32,
    /// Bound UDP socket used for both OSC and raw UDP transmission.
    pub udp_socket: Option<UdpSocket>,
    /// Destination IP address for OSC and UDP output. Defaults to
    /// `"127.0.0.1"`.
    pub ip: String,
    /// When `Some`, outgoing MIDI bytes are additionally forwarded as
    /// OSC packets to the given path, formatted for Plogue Bidule
    /// (e.g. `"/OSC_MIDI_0/MIDI"`).
    pub osc_midi_bidule: Option<String>,
}

impl std::fmt::Debug for MidiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiState")
            .field("device_name", &self.device_name)
            .field("input_device_name", &self.input_device_name)
            .field("output_index", &self.output_index)
            .field("input_index", &self.input_index)
            .field("cc_offset", &self.cc_offset)
            .field("ip", &self.ip)
            .field("osc_midi_bidule", &self.osc_midi_bidule)
            .field("stack_len", &self.stack.len())
            .field("cc_stack_len", &self.cc_stack.len())
            .finish_non_exhaustive()
    }
}

impl MidiState {
    /// Creates a new `MidiState`, opening the first available MIDI output device
    /// and binding a local UDP socket on an ephemeral port.
    pub fn new() -> Self {
        let mut state = Self {
            out: None,
            stack: Vec::new(),
            mono_stack: std::array::from_fn(|_| None),
            cc_stack: Vec::new(),
            osc: Osc::new(49162),
            udp: Udp::new(49161),
            cc_offset: 64,
            device_name: String::from("No Midi Device"),
            input_device_name: String::from("No Input Device"),
            output_index: -1,
            input_index: -1,
            udp_socket: UdpSocket::bind("0.0.0.0:0").ok(),
            ip: String::from("127.0.0.1"),
            osc_midi_bidule: None,
        };
        state.select_next_output();
        state
    }

    /// Advances [`output_index`](MidiState::output_index) to the next available
    /// device, wrapping around at the end of the list, and opens a new
    /// connection.
    pub fn select_next_output(&mut self) {
        if let Ok(midi) = MidiOutput::new("o2") {
            let ports = midi.ports();
            if ports.is_empty() {
                self.output_index = -1;
                self.device_name = String::from("No Output Device");
                self.out = None;
                return;
            }
            self.output_index = (self.output_index + 1) % ports.len() as i32;
            let port = &ports[self.output_index as usize];
            self.device_name = midi
                .port_name(port)
                .unwrap_or_else(|_| String::from("Unknown Device"));
            self.out = midi.connect(port, "o2-output").ok();
        }
    }

    /// Advances [`input_index`](MidiState::input_index) to the next available
    /// input device (display only; o2 does not process inbound MIDI).
    pub fn select_next_input(&mut self) {
        if let Ok(midi) = MidiInput::new("o2") {
            let ports = midi.ports();
            if ports.is_empty() {
                self.input_index = -1;
                self.input_device_name = String::from("No Input Device");
                return;
            }
            self.input_index = (self.input_index + 1) % ports.len() as i32;
            let port = &ports[self.input_index as usize];
            self.input_device_name = midi
                .port_name(port)
                .unwrap_or_else(|_| String::from("Unknown Device"));
        }
    }

    /// Sends a raw MIDI message to the active output connection.
    ///
    /// If [`osc_midi_bidule`](MidiState::osc_midi_bidule) is set, the
    /// same bytes are also transmitted as a three-integer OSC packet to
    /// [`ip`](MidiState::ip):[`osc.port`](Osc::port), zero-padded to
    /// three arguments as required by Bidule.
    pub fn send_midi_msg(&mut self, msg: &[u8]) {
        if let Some(conn) = self.out.as_mut() {
            let _ = conn.send(msg);
        }
        if let Some(bidule_path) = &self.osc_midi_bidule
            && let Some(sock) = &self.udp_socket
        {
            let mut args = Vec::with_capacity(3);
            for &b in msg {
                args.push(OscType::Int(b as i32));
            }
            while args.len() < 3 {
                args.push(OscType::Int(0));
            }
            let packet = OscPacket::Message(OscMessage {
                addr: bidule_path.clone(),
                args,
            });
            if let Ok(bytes) = encoder::encode(&packet) {
                let _ = sock.send_to(&bytes, (self.ip.as_str(), self.osc.port));
            }
        }
    }

    /// Processes all pending note and CC events for the current frame.
    ///
    /// For each note in the polyphonic stack:
    /// * Sends Note On if the note has not yet been played.
    /// * Sends Note Off and removes the note when its length reaches zero.
    ///
    /// After processing notes, all pending CC/PB, OSC, and UDP messages are
    /// dispatched and the queues are cleared.
    pub fn run(&mut self) {
        let mut to_send = Vec::new();

        self.stack.retain_mut(|note| {
            if !note.is_played {
                to_send.push(vec![0x90 + note.channel, note.note_id, note.velocity]);
                note.is_played = true;
            }
            if note.length < 1 {
                to_send.push(vec![0x80 + note.channel, note.note_id, 0]);
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
                        to_send.push(vec![0x80 + note.channel, note.note_id, 0]);
                    }
                    *slot = None;
                    continue;
                }
                if !note.is_played {
                    to_send.push(vec![0x90 + note.channel, note.note_id, note.velocity]);
                    note.is_played = true;
                }
                note.length = note.length.saturating_sub(1);
            }
        }

        for msg in &self.cc_stack {
            match msg {
                MidiMessage::Cc(cc) => {
                    let knob_val = self.cc_offset.saturating_add(cc.knob).min(127);
                    to_send.push(vec![0xB0 + cc.channel, knob_val, cc.value]);
                }
                MidiMessage::Pb(pb) => {
                    to_send.push(vec![0xE0 + pb.channel, pb.lsb, pb.msb]);
                }
            }
        }

        for msg in to_send {
            self.send_midi_msg(&msg);
        }

        self.osc.run(self.udp_socket.as_ref(), &self.ip);
        self.udp.run(self.udp_socket.as_ref(), &self.ip);
        self.cc_stack.clear();
    }

    /// Sends Note Off for every currently playing note and clears all stacks.
    ///
    /// Also transmits an All Notes Off (CC 123) on every channel to silence any
    /// notes that may have been missed.
    pub fn silence(&mut self) {
        let mut kill_notes = Vec::new();

        for note in &self.stack {
            if note.is_played {
                kill_notes.push(vec![0x80 + note.channel, note.note_id, 0]);
            }
        }
        for n in self.mono_stack.iter().flatten() {
            if n.is_played {
                kill_notes.push(vec![0x80 + n.channel, n.note_id, 0]);
            }
        }
        for ch in 0..16 {
            kill_notes.push(vec![0xB0 + ch, 123, 0]);
        }

        for msg in kill_notes {
            self.send_midi_msg(&msg);
        }

        self.stack.clear();
        self.mono_stack = std::array::from_fn(|_| None);
        self.cc_stack.clear();
        self.osc.stack.clear();
        self.udp.stack.clear();
    }

    /// Sends an optional Bank Select, Sub-bank Select, and Program Change on
    /// the given channel.
    ///
    /// Parameters that are `None` are silently skipped.
    pub fn send_pg(&mut self, channel: u8, bank: Option<u8>, sub: Option<u8>, pgm: Option<u8>) {
        if let Some(b) = bank {
            self.send_midi_msg(&[0xB0 + channel, 0, b]);
        }
        if let Some(s) = sub {
            self.send_midi_msg(&[0xB0 + channel, 32, s]);
        }
        if let Some(p) = pgm {
            self.send_midi_msg(&[0xC0 + channel, p.min(127)]);
        }
    }

    /// Transmits a MIDI Start message (0xFA) to the output device.
    pub fn send_clock_start(&mut self) {
        self.send_midi_msg(&[0xFA]);
    }

    /// Transmits a MIDI Stop message (0xFC) to the output device.
    pub fn send_clock_stop(&mut self) {
        self.send_midi_msg(&[0xFC]);
    }
}

impl Default for MidiState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_state_run_lifecycle() {
        let mut state = MidiState::new();

        state.stack.push(MidiNote {
            channel: 0,
            octave: 4,
            note: 'C',
            note_id: 60,
            velocity: 100,
            length: 2,
            is_played: false,
        });

        state.run();
        assert_eq!(state.stack.len(), 1);
        assert!(state.stack[0].is_played);
        assert_eq!(state.stack[0].length, 1);

        state.run();
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.stack[0].length, 0);

        state.run();
        assert_eq!(state.stack.len(), 0);
    }

    #[test]
    fn test_midi_state_silence_clears_all() {
        let mut state = MidiState::new();

        state.stack.push(MidiNote {
            channel: 15,
            octave: 2,
            note: 'A',
            note_id: 45,
            velocity: 127,
            length: 5,
            is_played: true,
        });
        state.mono_stack[5] = Some(MidiNote {
            channel: 5,
            octave: 3,
            note: 'B',
            note_id: 59,
            velocity: 64,
            length: 1,
            is_played: false,
        });
        state.cc_stack.push(MidiMessage::Cc(MidiCc {
            channel: 0,
            knob: 10,
            value: 127,
        }));
        state
            .osc
            .stack
            .push(("/test".to_string(), "data".to_string()));
        state.udp.stack.push("udp_data".to_string());

        state.silence();

        assert!(state.stack.is_empty());
        assert!(state.mono_stack.iter().all(|s| s.is_none()));
        assert!(state.cc_stack.is_empty());
        assert!(state.osc.stack.is_empty());
        assert!(state.udp.stack.is_empty());
    }

    #[test]
    fn test_midi_state_run_clears_transient_stacks() {
        let mut state = MidiState::new();

        state.cc_stack.push(MidiMessage::Pb(MidiPb {
            channel: 0,
            lsb: 0,
            msb: 0,
        }));
        state
            .osc
            .stack
            .push(("/path".to_string(), "body".to_string()));
        state.udp.stack.push("datagram".to_string());

        state.run();

        assert!(state.cc_stack.is_empty());
        assert!(state.osc.stack.is_empty());
        assert!(state.udp.stack.is_empty());
    }
}
