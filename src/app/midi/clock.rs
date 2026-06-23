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

use super::wire::{MidiCommand, MidiFrame, unpack};
use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::{
    net::UdpSocket,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
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
const PUPPET_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(not(target_os = "macos"))]
use midir::{MidiOutput, MidiOutputConnection};
#[cfg(not(target_os = "macos"))]
const SPIN_LEAD: Duration = Duration::from_millis(1);
#[cfg(not(target_os = "macos"))]
const ENGINE_LEAD: Duration = Duration::from_millis(8);

#[cfg(not(target_os = "macos"))]
pub(crate) struct Clock {
    out: Option<MidiOutputConnection>,
    osc_midi_bidule: Option<String>,
    ip: String,
    osc_port: u16,
    udp_port: u16,
    udp_socket: Option<UdpSocket>,
    in_rx: Receiver<u8>,
    engine_ticks: Arc<AtomicU32>,
    transport_event: Arc<AtomicU8>,
    is_puppet_shared: Arc<AtomicBool>,
}

#[cfg(not(target_os = "macos"))]
impl Clock {
    pub(crate) fn new(
        udp_socket: Option<UdpSocket>,
        osc_port: u16,
        udp_port: u16,
        in_rx: Receiver<u8>,
        engine_ticks: Arc<AtomicU32>,
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
            engine_ticks,
            transport_event,
            is_puppet_shared,
        }
    }

    pub(crate) fn run(
        mut self,
        shared: Arc<AtomicU64>,
        frame_rx: Receiver<MidiFrame>,
        cmd_rx: Receiver<MidiCommand>,
    ) {
        let mut next_pulse = Instant::now();
        let mut clock_counter: u8 = 0;
        let mut tick_issued = false;
        let mut puppet = false;
        let mut puppet_pulse: u8 = 0;
        let mut last_pulse = Instant::now();
        let mut current_bpm: u16 = 120;

        loop {
            let packed = shared.load(Ordering::Acquire);
            let (bpm, paused, bclock, stop) = unpack(packed);

            if stop {
                while let Ok(cmd) = cmd_rx.try_recv() {
                    self.exec_cmd(cmd, &frame_rx);
                }
                break;
            }

            if bpm != current_bpm {
                let new_rate =
                    Duration::from_nanos(60_000_000_000_u64 / (bpm.max(1) as u64) / 4 / 6);
                let now = Instant::now();
                if next_pulse > now + new_rate {
                    next_pulse = now + new_rate;
                }
                current_bpm = bpm;
            }

            let clock_rate =
                Duration::from_nanos(60_000_000_000_u64 / (current_bpm.max(1) as u64) / 4 / 6);

            while let Ok(byte) = self.in_rx.try_recv() {
                match byte {
                    MIDI_CLOCK_PULSE => {
                        last_pulse = Instant::now();
                        if !puppet {
                            puppet = true;
                            self.is_puppet_shared.store(true, Ordering::Relaxed);
                        }
                        puppet_pulse = (puppet_pulse + 1) % 6;
                        if !paused {
                            if puppet_pulse == 5 {
                                self.engine_ticks.fetch_add(1, Ordering::Relaxed);
                            } else if puppet_pulse == 0
                                && let Ok(frame) = frame_rx.try_recv()
                            {
                                self.dispatch_frame(frame);
                            }
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
                next_pulse = Instant::now() + ENGINE_LEAD;
                clock_counter = 0;
                tick_issued = false;
            }

            while let Ok(cmd) = cmd_rx.try_recv() {
                self.exec_cmd(cmd, &frame_rx);
            }

            if puppet {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }

            if paused {
                next_pulse = Instant::now() + ENGINE_LEAD;
                clock_counter = 0;
                tick_issued = false;
                if let Ok(frame) = frame_rx.recv_timeout(Duration::from_millis(5)) {
                    self.dispatch_frame(frame);
                }
                continue;
            }

            let now = Instant::now();
            let pulses_to_boundary = (6 - clock_counter as u32) % 6;
            let boundary = next_pulse + clock_rate * pulses_to_boundary;
            let tick_time = boundary.checked_sub(ENGINE_LEAD).unwrap_or(now);

            if !tick_issued && now >= tick_time {
                self.engine_ticks.fetch_add(1, Ordering::Relaxed);
                tick_issued = true;
            }

            if next_pulse > now + SPIN_LEAD {
                let mut wake = next_pulse - SPIN_LEAD;
                if !tick_issued && tick_time < wake {
                    wake = tick_time;
                }
                let chunk = wake
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(15));
                if let Ok(cmd) = cmd_rx.recv_timeout(chunk) {
                    self.exec_cmd(cmd, &frame_rx);
                }
                continue;
            }

            while Instant::now() < next_pulse {
                std::hint::spin_loop();
            }

            if bclock && let Some(conn) = self.out.as_mut() {
                let _ = conn.send(&[MIDI_CLOCK_PULSE]);
            }

            if clock_counter == 0 {
                if let Ok(frame) = frame_rx.try_recv() {
                    self.dispatch_frame(frame);
                }
                tick_issued = false;
            }

            clock_counter = (clock_counter + 1) % 6;
            next_pulse += clock_rate;

            let now = Instant::now();
            if now.duration_since(next_pulse) > clock_rate * 12 {
                next_pulse = now + clock_rate;
                clock_counter = 0;
                tick_issued = false;
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
            MidiCommand::Silence(offs) => {
                while frame_rx.try_recv().is_ok() {}
                for off in &offs {
                    self.send(off);
                }
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
            MidiCommand::CreateVirtualOutput(name) => {
                self.out = None;
                #[cfg(unix)]
                {
                    use midir::os::unix::VirtualOutput;
                    self.out = MidiOutput::new("o2")
                        .ok()
                        .and_then(|midi| midi.create_virtual(&name).ok());
                }
                #[cfg(not(unix))]
                let _ = name;
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

#[cfg(target_os = "macos")]
use coremidi::{Client, Destination, OutputPort, PacketBuffer, VirtualSource};
#[cfg(target_os = "macos")]
const ENGINE_LEAD_NANOS: u64 = 8_000_000;

#[cfg(target_os = "macos")]
#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioGetCurrentHostTime() -> u64;
    fn AudioConvertNanosToHostTime(in_nanos: u64) -> u64;
}

#[cfg(target_os = "macos")]
#[inline]
fn now_host() -> u64 {
    unsafe { AudioGetCurrentHostTime() }
}

#[cfg(target_os = "macos")]
#[inline]
fn nanos_to_host(nanos: u64) -> u64 {
    unsafe { AudioConvertNanosToHostTime(nanos) }
}

#[cfg(target_os = "macos")]
#[inline]
fn nanos_per_pulse(bpm: u16) -> u64 {
    60_000_000_000_u64 / (bpm.max(1) as u64) / 4 / 6
}

#[cfg(target_os = "macos")]
pub(crate) struct Clock {
    out_port: Option<OutputPort>,
    dest: Option<Destination>,
    virtual_src: Option<VirtualSource>,
    osc_midi_bidule: Option<String>,
    ip: String,
    osc_port: u16,
    udp_port: u16,
    udp_socket: Option<UdpSocket>,
    in_rx: Receiver<u8>,
    engine_ticks: Arc<AtomicU32>,
    transport_event: Arc<AtomicU8>,
    is_puppet_shared: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
impl Clock {
    pub(crate) fn new(
        udp_socket: Option<UdpSocket>,
        osc_port: u16,
        udp_port: u16,
        in_rx: Receiver<u8>,
        engine_ticks: Arc<AtomicU32>,
        transport_event: Arc<AtomicU8>,
        is_puppet_shared: Arc<AtomicBool>,
    ) -> Self {
        Self {
            out_port: None,
            dest: None,
            virtual_src: None,
            osc_midi_bidule: None,
            ip: String::from("127.0.0.1"),
            osc_port,
            udp_port,
            udp_socket,
            in_rx,
            engine_ticks,
            transport_event,
            is_puppet_shared,
        }
    }

    pub(crate) fn run(
        mut self,
        shared: Arc<AtomicU64>,
        frame_rx: Receiver<MidiFrame>,
        cmd_rx: Receiver<MidiCommand>,
    ) {
        let lead = nanos_to_host(ENGINE_LEAD_NANOS);

        let mut current_bpm: u16 = 120;
        let mut step = nanos_to_host(nanos_per_pulse(current_bpm));
        let mut next_pulse = now_host();
        let mut pulse_in_frame: u8 = 0;
        let mut next_boundary = next_pulse;
        let mut tick_issued = false;

        let mut puppet = false;
        let mut puppet_pulse: u8 = 0;
        let mut last_pulse = Instant::now();

        loop {
            let packed = shared.load(Ordering::Acquire);
            let (bpm, paused, bclock, stop) = unpack(packed);

            if stop {
                while let Ok(cmd) = cmd_rx.try_recv() {
                    self.exec_cmd(cmd, &frame_rx);
                }
                break;
            }

            if bpm != current_bpm {
                current_bpm = bpm;
                step = nanos_to_host(nanos_per_pulse(current_bpm));
                let now = now_host();
                if next_pulse > now + step {
                    next_pulse = now + step;
                }
                let to_boundary = ((6 - pulse_in_frame as u32) % 6) as u64;
                next_boundary = next_pulse + to_boundary * step;
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
                        if !paused {
                            if puppet_pulse == 5 {
                                self.engine_ticks.fetch_add(1, Ordering::Relaxed);
                            } else if puppet_pulse == 0
                                && let Ok(frame) = frame_rx.try_recv()
                            {
                                self.dispatch_frame(frame, 0);
                            }
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
                let now = now_host();
                next_pulse = now;
                pulse_in_frame = 0;
                next_boundary = now;
                tick_issued = false;
            }

            while let Ok(cmd) = cmd_rx.try_recv() {
                self.exec_cmd(cmd, &frame_rx);
            }

            if puppet {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }

            if paused {
                let now = now_host();
                next_pulse = now;
                pulse_in_frame = 0;
                next_boundary = now;
                tick_issued = false;
                if let Ok(frame) = frame_rx.recv_timeout(Duration::from_millis(5)) {
                    self.dispatch_frame(frame, 0);
                }
                continue;
            }

            let now = now_host();

            if next_pulse < now && now - next_pulse > 12 * step {
                next_pulse = now;
                pulse_in_frame = 0;
                next_boundary = now;
                tick_issued = false;
            }

            let horizon = now + lead;

            while next_pulse <= horizon {
                if bclock {
                    self.send_at(&[MIDI_CLOCK_PULSE], next_pulse);
                }
                pulse_in_frame = (pulse_in_frame + 1) % 6;
                next_pulse += step;
            }

            if !tick_issued && next_boundary <= horizon {
                self.engine_ticks.fetch_add(1, Ordering::Relaxed);
                tick_issued = true;
            }
            if tick_issued && let Ok(frame) = frame_rx.try_recv() {
                self.dispatch_frame(frame, next_boundary);
                tick_issued = false;
                next_boundary += 6 * step;
            }

            if let Ok(cmd) = cmd_rx.recv_timeout(Duration::from_millis(1)) {
                self.exec_cmd(cmd, &frame_rx);
            }
        }
    }

    fn dispatch_frame(&mut self, frame: MidiFrame, host_ts: u64) {
        self.osc_midi_bidule = frame.osc_midi_bidule;
        self.ip = frame.ip;
        self.osc_port = frame.osc_port;
        self.udp_port = frame.udp_port;

        if !frame.bytes.is_empty() {
            if let (Some(port), Some(dest)) = (self.out_port.as_ref(), self.dest.as_ref()) {
                let mut msgs = frame.bytes.iter();
                if let Some(first) = msgs.next() {
                    let mut buf = PacketBuffer::new(host_ts, first);
                    for msg in msgs {
                        buf.push_data(host_ts, msg);
                    }
                    let _ = port.send(dest, &buf);
                }
            }
            for msg in &frame.bytes {
                self.forward_bidule(msg);
            }
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
            MidiCommand::Silence(offs) => {
                while frame_rx.try_recv().is_ok() {}
                for off in &offs {
                    self.send_at(off, 0);
                }
                for ch in 0..MIDI_CHANNELS as u8 {
                    self.send_at(&[MIDI_CC + ch, MIDI_ALL_NOTES_OFF, 0], 0);
                }
            }
            MidiCommand::ClockStart => self.send_at(&[MIDI_START], 0),
            MidiCommand::ClockStop => self.send_at(&[MIDI_STOP], 0),
            MidiCommand::SelectOutput(idx) => {
                self.virtual_src = None;
                self.dest = if idx >= 0 {
                    Destination::from_index(idx as usize)
                } else {
                    None
                };
                if self.out_port.is_none()
                    && let Ok(client) = Client::new("o2")
                {
                    self.out_port = client.output_port("o2-output").ok();
                }
            }
            MidiCommand::CreateVirtualOutput(name) => {
                self.dest = None;
                self.virtual_src = Client::new("o2")
                    .ok()
                    .and_then(|client| client.virtual_source(&name).ok());
            }
            MidiCommand::SendPg {
                channel,
                bank,
                sub,
                pgm,
            } => {
                if let Some(b) = bank {
                    self.send_at(&[MIDI_CC + channel, 0, b], 0);
                }
                if let Some(s) = sub {
                    self.send_at(&[MIDI_CC + channel, MIDI_BANK_SELECT_LSB, s], 0);
                }
                if let Some(p) = pgm {
                    self.send_at(&[MIDI_PROGRAM_CHANGE + channel, p.min(127)], 0);
                }
            }
        }
    }

    fn send_at(&self, msg: &[u8], host_ts: u64) {
        let buf = PacketBuffer::new(host_ts, msg);
        if let (Some(port), Some(dest)) = (self.out_port.as_ref(), self.dest.as_ref()) {
            let _ = port.send(dest, &buf);
        }
        if let Some(src) = self.virtual_src.as_ref() {
            let _ = src.received(&buf);
        }
        self.forward_bidule(msg);
    }

    fn forward_bidule(&self, msg: &[u8]) {
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
