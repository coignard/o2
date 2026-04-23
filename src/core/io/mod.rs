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

/// MIDI output, note stacks, CC/PB messages, OSC, and UDP.
pub mod midi;

/// OSC output: packet encoding and per-tick dispatch.
pub mod osc;

/// UDP output: raw datagram dispatch.
pub mod udp;

pub use midi::{MidiCc, MidiMessage, MidiNote, MidiPb, MidiState};
pub use osc::Osc;
pub use udp::Udp;
