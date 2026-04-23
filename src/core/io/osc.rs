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

//! OSC output: packet construction and per-tick dispatch.
//!
//! Each frame, the `=` operator pushes `(path, body)` pairs onto
//! [`Osc::stack`]. [`Osc::run`] encodes them as OSC packets and sends
//! them over the shared UDP socket.

use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::net::UdpSocket;

/// Pending OSC message queue and destination port.
#[derive(Debug, Default)]
pub struct Osc {
    /// UDP port to which OSC packets are sent. Defaults to `49162`.
    pub port: u16,

    /// Pending `(path, body)` pairs queued by the `=` operator.
    /// Drained on every call to [`run`](Osc::run).
    pub stack: Vec<(String, String)>,
}

impl Osc {
    /// Creates a new [`Osc`] instance that will send packets to `port`.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            stack: Vec::new(),
        }
    }

    /// Encodes and transmits all pending OSC messages, then clears the stack.
    ///
    /// Each body character is converted to its base-36 integer value and
    /// packed as an `OscType::Int` argument. If `socket` is `None` the
    /// stack is cleared without sending.
    pub fn run(&mut self, socket: Option<&UdpSocket>, ip: &str) {
        let Some(sock) = socket else {
            self.stack.clear();
            return;
        };

        for (path, msg) in self.stack.drain(..) {
            let args: Vec<OscType> = msg
                .chars()
                .map(|c| OscType::Int(c.to_digit(36).unwrap_or(0) as i32))
                .collect();

            let packet = OscPacket::Message(OscMessage {
                addr: format!("/{}", path),
                args,
            });

            if let Ok(bytes) = encoder::encode(&packet) {
                let _ = sock.send_to(&bytes, (ip, self.port));
            }
        }
    }
}
