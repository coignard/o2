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

pub(crate) const DEFAULT_OSC_PORT: u16 = 49162;
pub(crate) const DEFAULT_UDP_PORT: u16 = 49161;

pub(crate) fn pack(bpm: u16, paused: bool, bclock: bool, stop: bool) -> u64 {
    (bpm as u64) | ((paused as u64) << 16) | ((bclock as u64) << 17) | ((stop as u64) << 18)
}

pub(crate) fn unpack(val: u64) -> (u16, bool, bool, bool) {
    (
        (val & 0xFFFF) as u16,
        (val >> 16) & 1 == 1,
        (val >> 17) & 1 == 1,
        (val >> 18) & 1 == 1,
    )
}

pub(crate) struct MidiFrame {
    pub(crate) bytes: Vec<Vec<u8>>,
    pub(crate) osc: Vec<(String, String)>,
    pub(crate) udp: Vec<String>,
    pub(crate) osc_port: u16,
    pub(crate) udp_port: u16,
    pub(crate) ip: String,
    pub(crate) osc_midi_bidule: Option<String>,
}

pub(crate) enum MidiCommand {
    Silence(Vec<[u8; 3]>),
    ClockStart,
    ClockStop,
    SelectOutput(i32),
    CreateVirtualOutput(String),
    SendPg {
        channel: u8,
        bank: Option<u8>,
        sub: Option<u8>,
        pgm: Option<u8>,
    },
}

#[cfg(test)]
mod tests {
    use super::{pack, unpack};

    #[test]
    fn pack_round_trips_through_unpack() {
        let cases = [
            (120u16, false, false, false),
            (360, true, false, false),
            (1, false, true, false),
            (200, true, true, true),
        ];
        for (bpm, paused, bclock, stop) in cases {
            assert_eq!(
                unpack(pack(bpm, paused, bclock, stop)),
                (bpm, paused, bclock, stop)
            );
        }
    }

    #[test]
    fn flags_are_independent() {
        let (_, paused, bclock, stop) = unpack(pack(120, true, false, true));
        assert!(paused);
        assert!(!bclock);
        assert!(stop);
    }
}
