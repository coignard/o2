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

use super::clock::Clock;
use super::wire::{DEFAULT_OSC_PORT, DEFAULT_UDP_PORT, MidiCommand, MidiFrame, pack};
#[cfg(not(target_os = "macos"))]
use midir::{MidiInput, MidiOutput};
use o2_rs::core::midi::{MIDI_NOTE_OFF, MidiEngine};
use std::{
    net::UdpSocket,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    thread::JoinHandle,
};

#[cfg(not(target_os = "macos"))]
type InputConn = midir::MidiInputConnection<()>;

#[cfg(target_os = "macos")]
type InputConn = coremidi::InputPort;

pub struct MidiState {
    pub engine: MidiEngine,
    pub osc_port: u16,
    pub udp_port: u16,
    pub device_name: String,
    pub input_device_name: String,
    pub output_index: i32,
    pub input_index: i32,
    pub is_virtual_output: bool,
    pub ip: String,
    pub osc_midi_bidule: Option<String>,

    frame_tx: SyncSender<MidiFrame>,
    cmd_tx: SyncSender<MidiCommand>,
    in_tx: SyncSender<u8>,
    _input_conn: Option<InputConn>,
    shared: Arc<AtomicU64>,
    engine_ticks: Arc<AtomicU32>,
    transport_event: Arc<AtomicU8>,
    is_puppet: Arc<AtomicBool>,
    _thread_handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for MidiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiState")
            .field("device_name", &self.device_name)
            .field("input_device_name", &self.input_device_name)
            .field("output_index", &self.output_index)
            .field("input_index", &self.input_index)
            .field("cc_offset", &self.engine.cc_offset)
            .field("ip", &self.ip)
            .field("osc_midi_bidule", &self.osc_midi_bidule)
            .field("stack_len", &self.engine.stack.len())
            .field("cc_stack_len", &self.engine.cc_stack.len())
            .finish_non_exhaustive()
    }
}

impl MidiState {
    pub fn new() -> Self {
        let udp_socket = UdpSocket::bind("0.0.0.0:0").ok();
        let shared = Arc::new(AtomicU64::new(pack(120, true, false, false)));
        let (frame_tx, frame_rx) = sync_channel::<MidiFrame>(64);
        let (cmd_tx, cmd_rx) = sync_channel::<MidiCommand>(16);
        let (in_tx, in_rx) = sync_channel::<u8>(64);
        let engine_ticks = Arc::new(AtomicU32::new(0));
        let transport_event = Arc::new(AtomicU8::new(0));
        let is_puppet = Arc::new(AtomicBool::new(false));

        let clock = Clock::new(
            udp_socket,
            DEFAULT_OSC_PORT,
            DEFAULT_UDP_PORT,
            in_rx,
            Arc::clone(&engine_ticks),
            Arc::clone(&transport_event),
            Arc::clone(&is_puppet),
        );
        let shared_clone = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("midi-clock".into())
            .spawn(move || clock.run(shared_clone, frame_rx, cmd_rx))
            .ok();

        let mut state = Self {
            engine: MidiEngine::new(),
            osc_port: DEFAULT_OSC_PORT,
            udp_port: DEFAULT_UDP_PORT,
            device_name: String::from("No Midi Device"),
            input_device_name: String::from("No Input Device"),
            output_index: -1,
            input_index: -1,
            is_virtual_output: false,
            ip: String::from("127.0.0.1"),
            osc_midi_bidule: None,
            frame_tx,
            cmd_tx,
            in_tx,
            _input_conn: None,
            shared,
            engine_ticks,
            transport_event,
            is_puppet,
            _thread_handle: handle,
        };
        state.select_next_output();
        state
    }

    pub fn set_shared(&self, bpm: usize, paused: bool, bclock: bool) {
        self.shared
            .store(pack(bpm as u16, paused, bclock, false), Ordering::Relaxed);
    }

    pub fn select_output_by_arg(&mut self, arg: &str) {
        if arg.is_empty() {
            self.select_output_by_index(0);
            return;
        }
        let ports = output_ports();
        if let Some(idx) = ports.iter().position(|name| name == arg) {
            self.select_output_by_index(idx as i32);
        } else if let Ok(idx) = arg.parse::<usize>() {
            self.select_output_by_index(idx as i32);
        } else {
            self.create_virtual_output(arg);
        }
    }

    pub fn create_virtual_output(&mut self, name: &str) {
        self.is_virtual_output = true;
        self.output_index = -1;
        self.device_name = name.to_string();
        let _ = self
            .cmd_tx
            .try_send(MidiCommand::CreateVirtualOutput(name.to_string()));
    }

    #[cfg(not(target_os = "macos"))]
    pub fn select_next_output(&mut self) {
        self.is_virtual_output = false;
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

    #[cfg(target_os = "macos")]
    pub fn select_next_output(&mut self) {
        self.is_virtual_output = false;
        let count = coremidi::Destinations::count();
        if count == 0 {
            self.output_index = -1;
            self.device_name = String::from("No Output Device");
        } else {
            self.output_index = (self.output_index + 1) % count as i32;
            self.device_name = coremidi::Destination::from_index(self.output_index as usize)
                .and_then(|d| d.display_name())
                .unwrap_or_else(|| String::from("Unknown Device"));
        }
        let _ = self
            .cmd_tx
            .try_send(MidiCommand::SelectOutput(self.output_index));
    }

    #[cfg(not(target_os = "macos"))]
    pub fn select_next_input(&mut self) {
        let count = MidiInput::new("o2")
            .map(|midi| midi.ports().len() as i32)
            .unwrap_or(0);
        let next = if self.input_index >= count - 1 {
            -1
        } else {
            self.input_index + 1
        };
        self.select_input_by_index(next);
    }

    #[cfg(not(target_os = "macos"))]
    pub fn select_input_by_index(&mut self, index: i32) {
        self._input_conn = None;

        if index < 0 {
            self.input_index = -1;
            self.input_device_name = String::from("No Input Device");
            return;
        }

        let Ok(midi) = MidiInput::new("o2") else {
            return;
        };
        let ports = midi.ports();
        let Some(port) = ports.get(index as usize) else {
            self.input_index = -1;
            self.input_device_name = String::from("No Input Device");
            return;
        };
        let name = midi
            .port_name(port)
            .unwrap_or_else(|_| String::from("Unknown Device"));
        let tx = self.in_tx.clone();

        match midi.connect(
            port,
            "o2-input",
            move |_, data, _| {
                if let Some(&byte) = data.first() {
                    let _ = tx.try_send(byte);
                }
            },
            (),
        ) {
            Ok(conn) => {
                self.input_index = index;
                self.input_device_name = name;
                self._input_conn = Some(conn);
            }
            Err(_) => {
                self.input_index = -1;
                self.input_device_name = String::from("No Input Device");
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn select_next_input(&mut self) {
        let count = coremidi::Sources::count() as i32;
        let next = if self.input_index >= count - 1 {
            -1
        } else {
            self.input_index + 1
        };
        self.select_input_by_index(next);
    }

    #[cfg(target_os = "macos")]
    pub fn select_input_by_index(&mut self, index: i32) {
        self._input_conn = None;

        if index < 0 {
            self.input_index = -1;
            self.input_device_name = String::from("No Input Device");
            return;
        }

        let Some(source) = coremidi::Source::from_index(index as usize) else {
            self.input_index = -1;
            self.input_device_name = String::from("No Input Device");
            return;
        };
        let name = source
            .display_name()
            .unwrap_or_else(|| String::from("Unknown Device"));
        let tx = self.in_tx.clone();

        let Ok(client) = coremidi::Client::new("o2") else {
            self.input_index = -1;
            self.input_device_name = String::from("No Input Device");
            return;
        };
        let port = match client.input_port("o2-input", move |packets: &coremidi::PacketList| {
            for packet in packets.iter() {
                for &byte in packet.data() {
                    let _ = tx.try_send(byte);
                }
            }
        }) {
            Ok(port) => port,
            Err(_) => {
                self.input_index = -1;
                self.input_device_name = String::from("No Input Device");
                return;
            }
        };

        if port.connect_source(&source).is_err() {
            self.input_index = -1;
            self.input_device_name = String::from("No Input Device");
            return;
        }

        self.input_index = index;
        self.input_device_name = name;
        self._input_conn = Some(port);
    }

    pub fn is_puppet(&self) -> bool {
        self.is_puppet.load(Ordering::Relaxed)
    }

    pub fn take_engine_tick(&self) -> bool {
        self.engine_ticks
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| c.checked_sub(1))
            .is_ok()
    }

    pub fn poll_transport_event(&self) -> u8 {
        self.transport_event.swap(0, Ordering::Relaxed)
    }

    pub fn flush(&mut self) {
        self.engine.last_io_count = self.engine.count_io();
        self.engine.run();
        let frame = MidiFrame {
            bytes: std::mem::take(&mut self.engine.pending),
            osc: std::mem::take(&mut self.engine.osc_buf),
            udp: std::mem::take(&mut self.engine.udp_buf),
            osc_port: self.osc_port,
            udp_port: self.udp_port,
            ip: self.ip.clone(),
            osc_midi_bidule: self.osc_midi_bidule.clone(),
        };
        if let Err(std::sync::mpsc::TrySendError::Full(frame)) = self.frame_tx.try_send(frame) {
            let _ = self.frame_tx.send(frame);
        }
    }

    pub fn silence(&mut self) {
        let mut offs: Vec<[u8; 3]> = Vec::new();
        for note in &self.engine.stack {
            offs.push([MIDI_NOTE_OFF + note.channel, note.note_id, 0]);
        }
        for note in self.engine.mono_stack.iter().flatten() {
            offs.push([MIDI_NOTE_OFF + note.channel, note.note_id, 0]);
        }
        self.engine.silence();
        let _ = self.cmd_tx.try_send(MidiCommand::Silence(offs));
    }

    pub fn send_clock_start(&self) {
        let _ = self.cmd_tx.try_send(MidiCommand::ClockStart);
    }

    pub fn send_clock_stop(&self) {
        let _ = self.cmd_tx.try_send(MidiCommand::ClockStop);
    }

    pub fn send_pg(&self, channel: u8, bank: Option<u8>, sub: Option<u8>, pgm: Option<u8>) {
        let _ = self.cmd_tx.try_send(MidiCommand::SendPg {
            channel,
            bank,
            sub,
            pgm,
        });
    }

    #[cfg(not(target_os = "macos"))]
    pub fn select_output_by_index(&mut self, index: i32) {
        self.is_virtual_output = false;
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

    #[cfg(target_os = "macos")]
    pub fn select_output_by_index(&mut self, index: i32) {
        self.is_virtual_output = false;
        if index < 0 {
            self.output_index = -1;
            self.device_name = String::from("No Output Device");
        } else if let Some(dest) = coremidi::Destination::from_index(index as usize) {
            self.output_index = index;
            self.device_name = dest
                .display_name()
                .unwrap_or_else(|| String::from("Unknown Device"));
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
        let val = self.shared.load(Ordering::Relaxed) | (1u64 << 18);
        self.shared.store(val, Ordering::Release);
        if let Some(handle) = self._thread_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_os = "macos")]
pub fn output_ports() -> Vec<String> {
    (0..coremidi::Destinations::count())
        .filter_map(|i| coremidi::Destination::from_index(i).and_then(|d| d.display_name()))
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn output_ports() -> Vec<String> {
    MidiOutput::new("o2")
        .map(|midi| {
            midi.ports()
                .iter()
                .filter_map(|p| midi.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default()
}
