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

//! Dedicated MIDI clock thread.
//!
//! [`MidiClock::run`] owns the MIDI output connection and runs a phase-locked
//! sleep-then-spin loop on a dedicated OS thread, completely isolated from
//! terminal rendering and keyboard input on the main thread.
//!
//! # Timing model
//!
//! Each clock interval is split into two phases:
//!
//! 1. **Sleep phase** — `thread::sleep(remaining − 1 ms)`. Yields the CPU
//!    cheaply while far from the target instant.
//! 2. **Spin phase** — tight `spin_loop` for the final millisecond. Achieves
//!    sub-10 μs precision without relying on OS timer resolution.
//!
//! MIDI frames (note / CC / OSC / UDP bytes) and control commands are drained
//! from their respective channels at the start of each sleep-wake, before the
//! spin begins, so they are dispatched within ~1 ms of being queued by the
//! main thread.

use crate::core::io::midi::{MidiCommand, MidiFrame};
use midir::{MidiOutput, MidiOutputConnection};
use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::{
    net::UdpSocket,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::{Duration, Instant},
};

const MIDI_CLOCK_PULSE: u8 = 0xF8;
const MIDI_START: u8 = 0xFA;
const MIDI_STOP: u8 = 0xFC;
const MIDI_CC: u8 = 0xB0;
const MIDI_ALL_NOTES_OFF: u8 = 123;
const MIDI_CHANNELS: usize = 16;
const MIDI_PROGRAM_CHANGE: u8 = 0xC0;
const MIDI_BANK_SELECT_LSB: u8 = 32;

/// Time budget reserved for the spin phase before each clock pulse.
const SPIN_LEAD: Duration = Duration::from_millis(1);

/// State owned exclusively by the MIDI clock thread.
pub(crate) struct MidiClock {
    out: Option<MidiOutputConnection>,
    osc_midi_bidule: Option<String>,
    ip: String,
    osc_port: u16,
    udp_port: u16,
    udp_socket: Option<UdpSocket>,
}

impl MidiClock {
    pub(crate) fn new(udp_socket: Option<UdpSocket>, osc_port: u16, udp_port: u16) -> Self {
        Self {
            out: None,
            osc_midi_bidule: None,
            ip: String::from("127.0.0.1"),
            osc_port,
            udp_port,
            udp_socket,
        }
    }

    /// Runs the clock loop. Blocks until the stop bit in `shared` is set.
    pub(crate) fn run(
        mut self,
        shared: Arc<AtomicU64>,
        frame_rx: Receiver<MidiFrame>,
        cmd_rx: Receiver<MidiCommand>,
    ) {
        let mut next_tick = Instant::now();
        let mut clock_counter: u8 = 0;

        loop {
            let (bpm, paused, bclock, stop) =
                crate::core::io::midi::unpack(shared.load(Ordering::Relaxed));

            if stop {
                break;
            }

            let tick_rate = Duration::from_nanos(60_000_000_000_u64 / (bpm.max(1) as u64) / 4);
            let clock_rate = tick_rate / 6;

            // Sleep until SPIN_LEAD before the target tick.
            let remaining = next_tick.saturating_duration_since(Instant::now());
            if remaining > SPIN_LEAD {
                std::thread::sleep(remaining - SPIN_LEAD);
            }

            // After waking: drain commands and frames before the spin window.
            while let Ok(cmd) = cmd_rx.try_recv() {
                self.exec_cmd(cmd, &frame_rx);
            }
            if !paused
                && clock_counter == 0
                && let Ok(frame) = frame_rx.try_recv()
            {
                self.dispatch_frame(frame);
            }

            // Spin until the exact target instant.
            while Instant::now() < next_tick {
                std::hint::spin_loop();
            }

            // Send clock pulse with minimal post-spin latency.
            if bclock
                && !paused
                && let Some(conn) = self.out.as_mut()
            {
                let _ = conn.send(&[MIDI_CLOCK_PULSE]);
            }

            clock_counter = (clock_counter + 1) % 6;
            next_tick += clock_rate;

            // Ant mill: reset phase if we fall more than 12 ticks behind
            // (e.g. after a system sleep or heavy load spike).
            let now = Instant::now();
            if now.duration_since(next_tick) > clock_rate * 12 {
                next_tick = now + clock_rate;
                clock_counter = 0;
            }
        }
    }

    fn dispatch_frame(&mut self, frame: MidiFrame) {
        self.osc_midi_bidule = frame.osc_midi_bidule;
        self.ip = frame.ip;
        self.osc_port = frame.osc_port;
        self.udp_port = frame.udp_port;

        for msg in &frame.bytes {
            self.send(msg);
        }

        if let Some(sock) = &self.udp_socket {
            for (path, body) in &frame.osc {
                let args: Vec<OscType> = body
                    .chars()
                    .map(|c| OscType::Int(c.to_digit(36).unwrap_or(0) as i32))
                    .collect();
                let packet = OscPacket::Message(OscMessage {
                    addr: format!("/{}", path),
                    args,
                });
                if let Ok(bytes) = encoder::encode(&packet) {
                    let _ = sock.send_to(&bytes, (self.ip.as_str(), self.osc_port));
                }
            }
            for msg in &frame.udp {
                let _ = sock.send_to(msg.as_bytes(), (self.ip.as_str(), self.udp_port));
            }
        }
    }

    fn exec_cmd(&mut self, cmd: MidiCommand, frame_rx: &Receiver<MidiFrame>) {
        match cmd {
            MidiCommand::Silence => {
                // Discard any queued frames so their Note Ons are never sent.
                while frame_rx.try_recv().is_ok() {}
                for ch in 0..MIDI_CHANNELS as u8 {
                    self.send(&[MIDI_CC + ch, MIDI_ALL_NOTES_OFF, 0]);
                }
            }
            MidiCommand::ClockStart => self.send(&[MIDI_START]),
            MidiCommand::ClockStop => self.send(&[MIDI_STOP]),
            MidiCommand::SelectOutput(idx) => {
                self.out = None;
                if idx >= 0
                    && let Ok(midi) = MidiOutput::new("o2")
                {
                    let ports = midi.ports();
                    if let Some(port) = ports.get(idx as usize) {
                        self.out = midi.connect(port, "o2-output").ok();
                    }
                }
            }
            MidiCommand::SendPg {
                channel,
                bank,
                sub,
                pgm,
            } => {
                if let Some(b) = bank {
                    self.send(&[MIDI_CC + channel, 0, b]);
                }
                if let Some(s) = sub {
                    self.send(&[MIDI_CC + channel, MIDI_BANK_SELECT_LSB, s]);
                }
                if let Some(p) = pgm {
                    self.send(&[MIDI_PROGRAM_CHANGE + channel, p.min(127)]);
                }
            }
        }
    }

    /// Sends raw bytes to the MIDI output and optionally to the Bidule OSC bridge.
    fn send(&mut self, msg: &[u8]) {
        if let Some(conn) = self.out.as_mut() {
            let _ = conn.send(msg);
        }
        if let Some(path) = &self.osc_midi_bidule
            && let Some(sock) = &self.udp_socket
        {
            let mut args: Vec<OscType> = msg.iter().map(|&b| OscType::Int(b as i32)).collect();
            while args.len() < 3 {
                args.push(OscType::Int(0));
            }
            let packet = OscPacket::Message(OscMessage {
                addr: path.clone(),
                args,
            });
            if let Ok(bytes) = encoder::encode(&packet) {
                let _ = sock.send_to(&bytes, (self.ip.as_str(), self.osc_port));
            }
        }
    }
}
