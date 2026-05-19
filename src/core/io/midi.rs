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

//! MIDI output facade, note stacks, and inter-thread communication types.
//!
//! [`MidiState`] runs on the main thread and acts as a facade over the
//! dedicated MIDI clock thread (see [`super::clock`]). Operators write note
//! and CC events into the local stacks exactly as before; [`MidiState::flush`]
//! processes those stacks and delivers a [`MidiFrame`] to the clock thread,
//! which dispatches the bytes at its next frame tick — synchronized with the
//! 0xF8 Beat Clock pulse stream.
//!
//! # Thread model
//!
//! ```text
//! main thread                          midi-clock thread
//! ─────────────────                    ─────────────────────────────
//! operate()  ──> fills stacks          phase-locked spin loop
//! flush()    ──> MidiFrame ──chan──>   dispatch_frame() + clock pulse
//! set_shared()  AtomicU64 ──────────> reads bpm / paused / bclock
//! MidiCommand   ──cmd-chan────────>    exec_cmd()
//! ```

use crate::core::io::clock::MidiClock;
use crate::core::io::osc::Osc;
use crate::core::io::udp::Udp;
use midir::{MidiInput, MidiOutput};
use std::{
    net::UdpSocket,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    thread::JoinHandle,
};

pub(crate) const MIDI_NOTE_OFF: u8 = 0x80;
const MIDI_NOTE_ON: u8 = 0x90;
const MIDI_CC: u8 = 0xB0;
const MIDI_PITCH_BEND: u8 = 0xE0;

const MIDI_CHANNELS: usize = 16;

pub(crate) const DEFAULT_OSC_PORT: u16 = 49162;
pub(crate) const DEFAULT_UDP_PORT: u16 = 49161;
const DEFAULT_CC_OFFSET: u8 = 64;

/// Packs BPM and flag bits into a single `u64` for atomic exchange.
///
/// Layout (little-endian):
/// - bits 0–15: BPM (u16)
/// - bit 16: paused
/// - bit 17: bclock enabled
/// - bit 18: stop (MIDI thread should exit)
pub(crate) fn pack(bpm: u16, paused: bool, bclock: bool, stop: bool) -> u64 {
    (bpm as u64) | ((paused as u64) << 16) | ((bclock as u64) << 17) | ((stop as u64) << 18)
}

/// Unpacks the value written by [`pack`] into `(bpm, paused, bclock, stop)`.
pub(crate) fn unpack(val: u64) -> (u16, bool, bool, bool) {
    (
        (val & 0xFFFF) as u16,
        (val >> 16) & 1 == 1,
        (val >> 17) & 1 == 1,
        (val >> 18) & 1 == 1,
    )
}

/// A bundle of MIDI, OSC, and UDP output produced by one engine frame.
///
/// Sent from the main thread to the clock thread via a bounded channel.
/// The clock thread dispatches the contents at its next frame tick,
/// keeping note output synchronized with the Beat Clock.
pub(crate) struct MidiFrame {
    /// Raw MIDI byte messages to send in order (Note On, Note Off, CC, PB…).
    pub(crate) bytes: Vec<Vec<u8>>,
    /// OSC `(path, body)` pairs from the `=` operator.
    pub(crate) osc: Vec<(String, String)>,
    /// UDP datagrams from the `;` operator.
    pub(crate) udp: Vec<String>,
    pub(crate) osc_port: u16,
    pub(crate) udp_port: u16,
    pub(crate) ip: String,
    pub(crate) osc_midi_bidule: Option<String>,
}

/// Commands sent from the main thread to the clock thread for control operations.
pub(crate) enum MidiCommand {
    /// Send All Notes Off on every channel and discard queued frames.
    Silence,
    /// Send MIDI Start (0xFA).
    ClockStart,
    /// Send MIDI Stop (0xFC).
    ClockStop,
    /// Reopen the MIDI output connection to the device at the given index
    /// (`-1` = disconnect).
    SelectOutput(i32),
    /// Send a Program Change (with optional Bank Select) on `channel`.
    SendPg {
        channel: u8,
        bank: Option<u8>,
        sub: Option<u8>,
        pgm: Option<u8>,
    },
}

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

/// Main-thread MIDI facade: accumulates note/CC events from operators and
/// communicates with the [`MidiClock`] thread.
pub struct MidiState {
    /// Polyphonic note stack: notes are added by `:` and removed after their
    /// `length` counts down to zero.
    pub stack: Vec<MidiNote>,
    /// Monophonic per-channel slots populated by `%`. Each channel can hold at
    /// most one active note at a time.
    pub mono_stack: [Option<MidiNote>; MIDI_CHANNELS],
    /// Pending CC and Pitch Bend messages, cleared after each [`flush`](MidiState::flush) call.
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
    /// Destination IP address for OSC and UDP output. Defaults to `"127.0.0.1"`.
    pub ip: String,
    /// When `Some`, outgoing MIDI bytes are additionally forwarded as OSC
    /// packets to the given path, formatted for Plogue Bidule.
    pub osc_midi_bidule: Option<String>,

    /// Bytes accumulated by [`run`](MidiState::run) during the current frame,
    /// drained into a [`MidiFrame`] by [`flush`](MidiState::flush).
    pending: Vec<Vec<u8>>,
    /// Sends completed frames to the clock thread.
    frame_tx: SyncSender<MidiFrame>,
    /// Sends control commands to the clock thread.
    cmd_tx: SyncSender<MidiCommand>,
    /// Shared atomic carrying live BPM, paused, bclock, and stop flags.
    shared: Arc<AtomicU64>,
    /// Join handle for the clock thread; taken on drop.
    _thread_handle: Option<JoinHandle<()>>,
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
    /// Creates a new `MidiState`, spawns the MIDI clock thread, and opens the
    /// first available MIDI output device.
    pub fn new() -> Self {
        let udp_socket = UdpSocket::bind("0.0.0.0:0").ok();
        let shared = Arc::new(AtomicU64::new(pack(120, true, false, false)));
        let (frame_tx, frame_rx) = sync_channel::<MidiFrame>(2);
        let (cmd_tx, cmd_rx) = sync_channel::<MidiCommand>(16);

        let clock = MidiClock::new(udp_socket, DEFAULT_OSC_PORT, DEFAULT_UDP_PORT);
        let shared_clone = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("midi-clock".into())
            .spawn(move || clock.run(shared_clone, frame_rx, cmd_rx))
            .ok();

        let mut state = Self {
            stack: Vec::new(),
            mono_stack: std::array::from_fn(|_| None),
            cc_stack: Vec::new(),
            osc: Osc::new(DEFAULT_OSC_PORT),
            udp: Udp::new(DEFAULT_UDP_PORT),
            cc_offset: DEFAULT_CC_OFFSET,
            device_name: String::from("No Midi Device"),
            input_device_name: String::from("No Input Device"),
            output_index: -1,
            input_index: -1,
            ip: String::from("127.0.0.1"),
            osc_midi_bidule: None,
            pending: Vec::new(),
            frame_tx,
            cmd_tx,
            shared,
            _thread_handle: handle,
        };
        state.select_next_output();
        state
    }

    /// Updates the shared atomic so the clock thread picks up the latest
    /// BPM, pause state, and Beat Clock flag on its next iteration.
    ///
    /// Call this after any change to `app.bpm`, `app.paused`, or
    /// `app.midi_bclock`, and once per main-loop iteration so the clock
    /// thread's timing stays in sync.
    pub fn set_shared(&self, bpm: usize, paused: bool, bclock: bool) {
        self.shared
            .store(pack(bpm as u16, paused, bclock, false), Ordering::Relaxed);
    }

    /// Advances [`output_index`](MidiState::output_index) to the next available
    /// device (wrapping), updates the display name, and sends a
    /// [`MidiCommand::SelectOutput`] to the clock thread to reopen the
    /// connection.
    pub fn select_next_output(&mut self) {
        if let Ok(midi) = MidiOutput::new("o2") {
            let ports = midi.ports();
            if ports.is_empty() {
                self.output_index = -1;
                self.device_name = String::from("No Output Device");
            } else {
                self.output_index = (self.output_index + 1) % ports.len() as i32;
                let port = &ports[self.output_index as usize];
                self.device_name = midi
                    .port_name(port)
                    .unwrap_or_else(|_| String::from("Unknown Device"));
            }
        }
        let _ = self
            .cmd_tx
            .try_send(MidiCommand::SelectOutput(self.output_index));
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

    /// Queues a raw MIDI message for inclusion in the next [`MidiFrame`].
    ///
    /// Called by operators (for immediate Note Off on note replacement) and
    /// internally by [`run`](MidiState::run) for Note On / Note Off / CC / PB.
    /// The bytes are dispatched by the clock thread at its next frame tick.
    pub fn send_midi_msg(&mut self, msg: &[u8]) {
        self.pending.push(msg.to_vec());
    }

    /// Processes all pending note and CC events for the current frame,
    /// populating [`pending`](MidiState::pending) with the resulting bytes.
    ///
    /// For each note in the polyphonic stack:
    /// - Queues Note On if the note has not yet been played.
    /// - Queues Note Off and removes the note when its length reaches zero.
    ///
    /// After processing notes, all pending CC/PB messages are processed and
    /// the CC stack is cleared.
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

    /// Processes the current frame's note/CC events via [`run`], then packages
    /// the resulting bytes together with any pending OSC and UDP messages into
    /// a [`MidiFrame`] and sends it to the clock thread.
    ///
    /// The clock thread dispatches the frame at its next frame tick (every sixth
    /// clock sub-tick), keeping note output aligned with the Beat Clock.
    /// If the clock thread's channel is full the frame is silently dropped
    /// (the clock thread has fallen more than two frames behind — extremely rare).
    pub fn flush(&mut self) {
        self.run();
        let frame = MidiFrame {
            bytes: std::mem::take(&mut self.pending),
            osc: std::mem::take(&mut self.osc.stack),
            udp: std::mem::take(&mut self.udp.stack),
            osc_port: self.osc.port,
            udp_port: self.udp.port,
            ip: self.ip.clone(),
            osc_midi_bidule: self.osc_midi_bidule.clone(),
        };
        let _ = self.frame_tx.try_send(frame);
    }

    /// Clears all local note/CC/OSC/UDP stacks and sends a
    /// [`MidiCommand::Silence`] to the clock thread, which drains any queued
    /// frames and transmits All Notes Off on every channel.
    pub fn silence(&mut self) {
        self.stack.clear();
        self.mono_stack = std::array::from_fn(|_| None);
        self.cc_stack.clear();
        self.osc.stack.clear();
        self.udp.stack.clear();
        self.pending.clear();
        let _ = self.cmd_tx.try_send(MidiCommand::Silence);
    }

    /// Sends a MIDI Start message (0xFA) via the clock thread.
    pub fn send_clock_start(&self) {
        let _ = self.cmd_tx.try_send(MidiCommand::ClockStart);
    }

    /// Sends a MIDI Stop message (0xFC) via the clock thread.
    pub fn send_clock_stop(&self) {
        let _ = self.cmd_tx.try_send(MidiCommand::ClockStop);
    }

    /// Sends an optional Bank Select, Sub-bank Select, and Program Change on
    /// the given channel via the clock thread.
    pub fn send_pg(&self, channel: u8, bank: Option<u8>, sub: Option<u8>, pgm: Option<u8>) {
        let _ = self.cmd_tx.try_send(MidiCommand::SendPg {
            channel,
            bank,
            sub,
            pgm,
        });
    }

    /// Instructs the clock thread to open the MIDI output device at `index`.
    ///
    /// Also updates the display name and index on the main thread for the UI.
    pub fn select_output_by_index(&mut self, index: i32) {
        if index < 0 {
            self.output_index = -1;
            self.device_name = String::from("No Output Device");
        } else if let Ok(midi) = MidiOutput::new("o2") {
            let ports = midi.ports();
            if let Some(port) = ports.get(index as usize) {
                self.output_index = index;
                self.device_name = midi
                    .port_name(port)
                    .unwrap_or_else(|_| String::from("Unknown Device"));
            }
        }
        let _ = self
            .cmd_tx
            .try_send(MidiCommand::SelectOutput(self.output_index));
    }
}

impl Default for MidiState {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MidiState {
    fn drop(&mut self) {
        // Set the stop bit so the clock thread exits its loop.
        let val = self.shared.load(Ordering::Relaxed) | (1u64 << 18);
        self.shared.store(val, Ordering::Release);
        // Join the thread to ensure clock-stop and silence are fully transmitted
        // before the process tears down the MIDI connection.
        if let Some(handle) = self._thread_handle.take() {
            let _ = handle.join();
        }
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

        state.flush();

        assert!(state.cc_stack.is_empty());
        assert!(state.osc.stack.is_empty());
        assert!(state.udp.stack.is_empty());
    }
}
