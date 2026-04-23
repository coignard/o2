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

//! UDP output: raw datagram dispatch.
//!
//! Each frame, the `;` operator pushes message strings onto
//! [`Udp::stack`]. [`Udp::run`] sends them as raw byte datagrams and
//! clears the queue.

use std::net::UdpSocket;

/// Pending UDP datagram queue and destination port.
#[derive(Debug, Default)]
pub struct Udp {
    /// UDP port to which datagrams are sent. Defaults to `49161`.
    pub port: u16,
    /// Pending raw message strings queued by the `;` operator.
    /// Drained on every call to [`run`](Udp::run).
    pub stack: Vec<String>,
}

impl Udp {
    /// Creates a new [`Udp`] instance that will send datagrams to `port`.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            stack: Vec::new(),
        }
    }

    /// Transmits all pending datagrams as raw UTF-8 bytes, then clears
    /// the stack. If `socket` is `None` the stack is cleared without
    /// sending.
    pub fn run(&mut self, socket: Option<&UdpSocket>, ip: &str) {
        let Some(sock) = socket else {
            self.stack.clear();
            return;
        };

        for msg in self.stack.drain(..) {
            let _ = sock.send_to(msg.as_bytes(), (ip, self.port));
        }
    }
}
