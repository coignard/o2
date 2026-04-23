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

//! # O₂
//!
//! `o2` is a Rust port of the [ORCΛ](https://github.com/hundredrabbits/Orca)
//! esoteric programming language and terminal livecoding environment.
//!
//! This library exposes the core engine of `o2`, allowing other developers to:
//! - Run the grid simulation ([`core::app`], [`core::vm`]).
//! - Send MIDI, OSC, and UDP output ([`core::io`]).
//! - Map glyphs to MIDI note IDs ([`core::transpose`]).
//! - Control tempo programmatically ([`editor::clock`]).
//! - Issue commander text commands ([`editor::commander`]).
//!
//! # Architecture
//!
//! The pipeline flows as follows:
//!
//! ```text
//! Grid cells (Vec<char>)
//!     → core::vm::run()                    per operator per frame
//!     → core::app::EditorState::operate()  full-frame tick
//!     → core::io::MidiState::run()         flush MIDI / OSC / UDP
//!     → ui::render::draw()                 render to terminal
//! ```

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

/// Grid simulation engine, MIDI I/O, and operator dispatch.
pub mod core;

/// Cursor, history, commander, clock, and input handling.
pub mod editor;

/// Terminal rendering and colour theme.
pub mod ui;
