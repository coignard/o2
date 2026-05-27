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
//!
//! # External clock (puppet mode)
//!
//! When an input device is selected and the connected source sends MIDI Beat
//! Clock pulses (0xF8), the thread switches to **puppet mode**: it counts
//! incoming pulses instead of running its own timer, dispatches note frames
//! every 6 pulses (= 1 engine tick), and sets `puppet_tick` so the main
//! thread calls `operate()` in sync with the external source.
//!
//! Puppet mode is exited automatically if no pulse arrives for 2 seconds,
//! after which the internal timer resumes.

use crate::core::io::midi::{MidiCommand, MidiFrame};
use midir::{MidiOutput, MidiOutputConnection};
use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::{
    net::UdpSocket,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::{Duration, Instant},
};

const MIDI_CLOCK_PULSE: u8 = 0xF8;
const MIDI_START: u8 = 0xFA;
const MIDI_CONTINUE: u8 = 0xFB;
const MIDI_STOP: u8 = 0xFC;
const MIDI_CC: u8 = 0xB0;
const MIDI_ALL_NOTES_OFF: u8 = 123;
const MIDI_CHANNELS: usize = 16;
const MIDI_PROGRAM_CHANGE: u8 = 0xC0;
const MIDI_BANK_SELECT_LSB: u8 = 32;

/// Time budget reserved for the spin phase before each clock pulse.
const SPIN_LEAD: Duration = Duration::from_millis(1);

/// Inactivity threshold before puppet mode is abandoned.
const PUPPET_TIMEOUT: Duration = Duration::from_secs(2);

/// State owned exclusively by the MIDI clock thread.
pub(crate) struct MidiClock {
    out: Option<MidiOutputConnection>,
    osc_midi_bidule: Option<String>,
    ip: String,
    osc_port: u16,
    udp_port: u16,
    udp_socket: Option<UdpSocket>,
    /// Incoming MIDI bytes from the selected input device.
    in_rx: Receiver<u8>,
    /// Set every 6 incoming 0xF8 pulses so the main thread can call `operate()`.
    puppet_tick: Arc<AtomicBool>,
    /// Carries the latest incoming transport byte (0xFA / 0xFB / 0xFC) for the main thread.
    transport_event: Arc<AtomicU8>,
    /// Whether external clock is currently controlling timing.
    is_puppet_shared: Arc<AtomicBool>,
}

impl MidiClock {
    pub(crate) fn new(
        udp_socket: Option<UdpSocket>,
        osc_port: u16,
        udp_port: u16,
        in_rx: Receiver<u8>,
        puppet_tick: Arc<AtomicBool>,
        transport_event: Arc<AtomicU8>,
        is_puppet_shared: Arc<AtomicBool>,
    ) -> Self {
        Self {
            out: None,
            osc_midi_bidule: None,
            ip: String::from("127.0.0.1"),
            osc_port,
            udp_port,
            udp_socket,
            in_rx,
            puppet_tick,
            transport_event,
            is_puppet_shared,
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
        let mut puppet = false;
        let mut puppet_pulse: u8 = 0;
        let mut last_pulse = Instant::now();
        let mut current_bpm = 120;

        loop {
            let packed = shared.load(Ordering::Relaxed);
            let (bpm, paused, bclock, stop) = crate::core::io::midi::unpack(packed);

            if stop {
                break;
            }

            if bpm != current_bpm {
                let new_tick_rate = Duration::from_nanos(60_000_000_000_u64 / (bpm.max(1) as u64) / 4);
                let new_clock_rate = new_tick_rate / 6;
                let now = Instant::now();
                if next_tick > now + new_clock_rate {
                    next_tick = now + new_clock_rate;
                }
                current_bpm = bpm;
            }

            while let Ok(byte) = self.in_rx.try_recv() {
                match byte {
                    MIDI_CLOCK_PULSE => {
                        last_pulse = Instant::now();
                        if !puppet {
                            puppet = true;
                            self.is_puppet_shared.store(true, Ordering::Relaxed);
                        }
                        puppet_pulse = (puppet_pulse + 1) % 6;
                        if puppet_pulse == 0 && !paused {
                            if let Ok(frame) = frame_rx.try_recv() {
                                self.dispatch_frame(frame);
                            }
                            self.puppet_tick.store(true, Ordering::Relaxed);
                        }
                    }
                    MIDI_START | MIDI_CONTINUE | MIDI_STOP => {
                        self.transport_event.store(byte, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }

            if puppet && last_pulse.elapsed() > PUPPET_TIMEOUT {
                puppet = false;
                self.is_puppet_shared.store(false, Ordering::Relaxed);
                next_tick = Instant::now();
                clock_counter = 0;
            }

            if puppet {
                while let Ok(cmd) = cmd_rx.try_recv() {
                    self.exec_cmd(cmd, &frame_rx);
                }
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }

            let tick_rate = Duration::from_nanos(60_000_000_000_u64 / (current_bpm.max(1) as u64) / 4);
            let clock_rate = tick_rate / 6;

            let now = Instant::now();
            if next_tick > now + SPIN_LEAD {
                let sleep_dur = (next_tick - now) - SPIN_LEAD;
                let chunk = sleep_dur.min(Duration::from_millis(15));
                if let Ok(cmd) = cmd_rx.recv_timeout(chunk) {
                    self.exec_cmd(cmd, &frame_rx);
                }
                continue;
            }

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
