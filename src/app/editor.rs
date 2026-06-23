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

use crate::app::clipboard;
use crate::app::history::History;
use crate::app::midi::MidiState;
use crate::app::types::{CommanderState, CursorState, InputMode, PopupType};
use o2_rs::core::oxygen::{OxygenEngine, StyleType};
use std::path::PathBuf;

pub struct EditorState {
    pub o2: OxygenEngine,

    pub grid_w: usize,
    pub grid_h: usize,
    pub scroll_x: usize,
    pub scroll_y: usize,

    pub cursor: CursorState,
    pub mode: InputMode,
    pub commander: CommanderState,

    pub paused: bool,
    pub running: bool,
    pub bpm: usize,
    pub bpm_target: usize,
    pub midi_bclock: bool,

    pub mouse_from: Option<(usize, usize)>,
    pub last_input_was_mouse: bool,

    pub current_file: Option<PathBuf>,
    pub history: History,
    pub midi: MidiState,

    pub popup: Vec<PopupType>,

    pub bw: bool,
    pub contrast: bool,
    pub guide: bool,
    pub custom_colors: [Option<(u8, u8, u8)>; 3],

    // ROFL BUFFER!!!
    pub rofl_buffer: String,
}

impl EditorState {
    pub fn new(w: usize, h: usize, seed: u64, undo_limit: usize) -> Self {
        let history = History::with_limit(undo_limit);

        let mut app = Self {
            o2: OxygenEngine::new(w, h, seed),
            grid_w: 8,
            grid_h: 8,
            scroll_x: 0,
            scroll_y: 0,
            cursor: CursorState::new(),
            mode: InputMode::Normal,
            paused: false,
            running: true,
            commander: CommanderState::new(),
            bpm: 120,
            bpm_target: 120,
            mouse_from: None,
            last_input_was_mouse: false,
            current_file: None,
            history,
            midi: MidiState::new(),
            midi_bclock: false,
            popup: Vec::new(),
            bw: false,
            contrast: false,
            guide: true,
            custom_colors: [None, None, None],
            rofl_buffer: String::with_capacity(4),
        };
        app.cursor.calc_bounds();
        app.history.record(&app.o2.cells);
        app.history.saved_absolute_index = Some(app.history.offset + app.history.index);
        app
    }

    pub fn update_scroll(&mut self, viewport_w: usize, viewport_h: usize) {
        if self.cursor.cx < self.scroll_x {
            self.scroll_x = self.cursor.cx;
        } else if self.cursor.cx >= self.scroll_x + viewport_w {
            self.scroll_x = self.cursor.cx + 1 - viewport_w;
        }

        if self.cursor.cy < self.scroll_y {
            self.scroll_y = self.cursor.cy;
        } else if self.cursor.cy >= self.scroll_y + viewport_h {
            self.scroll_y = self.cursor.cy + 1 - viewport_h;
        }

        let max_scroll_x = self.o2.w.saturating_sub(viewport_w);
        let max_scroll_y = self.o2.h.saturating_sub(viewport_h);
        self.scroll_x = self.scroll_x.min(max_scroll_x);
        self.scroll_y = self.scroll_y.min(max_scroll_y);
    }

    pub fn content_bounds(&self) -> (usize, usize) {
        let mut max_x = 0;
        let mut max_y = 0;
        for (i, &c) in self.o2.cells.iter().enumerate() {
            if c != '.' {
                max_x = max_x.max(i % self.o2.w);
                max_y = max_y.max(i / self.o2.w);
            }
        }
        (max_x + 1, max_y + 1)
    }

    pub fn load(&mut self, content: &str, path: Option<PathBuf>) {
        self.current_file = path;
        self.o2.load_grid(content);

        self.history.clear();
        self.history.record(&self.o2.cells);
        self.history.saved_absolute_index = Some(self.history.offset + self.history.index);
        self.select(
            self.cursor.cx as isize,
            self.cursor.cy as isize,
            self.cursor.cw,
            self.cursor.ch,
        );
    }

    pub fn to_grid_string(&self) -> String {
        let mut content = String::with_capacity((self.o2.w + 1) * self.o2.h);
        for y in 0..self.o2.h {
            for x in 0..self.o2.w {
                content.push(self.o2.cells[y * self.o2.w + x]);
            }
            content.push('\n');
        }
        content
    }

    pub fn save(&mut self) -> bool {
        let path = self
            .current_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("untitled.o2"));
        let content = self.to_grid_string();
        let success = std::fs::write(path, content.trim_end()).is_ok();
        if success {
            self.history.saved_absolute_index = Some(self.history.offset + self.history.index);
        }
        success
    }

    pub fn is_dirty(&self) -> bool {
        self.history
            .saved_absolute_index
            .is_none_or(|saved| saved != (self.history.offset + self.history.index))
    }

    pub fn undo(&mut self) {
        self.history.undo(&mut self.o2.cells);
    }

    pub fn redo(&mut self) {
        self.history.redo(&mut self.o2.cells);
    }

    pub fn is_allowed(g: char) -> bool {
        o2_rs::core::glyph::is_allowed(g)
    }

    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        let (bounds_w, bounds_h) = self.content_bounds();

        let min_w = bounds_w.max(self.cursor.max_x + 1).max(self.cursor.cx + 1);
        let min_h = bounds_h.max(self.cursor.max_y + 1).max(self.cursor.cy + 1);

        let final_w = new_w.max(min_w).max(1);
        let final_h = new_h.max(min_h).max(1);

        if final_w == self.o2.w && final_h == self.o2.h {
            return;
        }

        let mut new_cells = vec!['.'; final_w * final_h];
        let mut new_locks = vec![false; final_w * final_h];
        let mut new_ports = vec![None; final_w * final_h];
        let mut new_port_names = vec![None; final_w * final_h];

        for y in 0..self.o2.h.min(final_h) {
            for x in 0..self.o2.w.min(final_w) {
                let old_idx = y * self.o2.w + x;
                let new_idx = y * final_w + x;
                new_cells[new_idx] = self.o2.cells[old_idx];
                new_locks[new_idx] = self.o2.locks[old_idx];
                new_ports[new_idx] = self.o2.ports[old_idx];
                new_port_names[new_idx] = self.o2.port_names[old_idx];
            }
        }

        self.o2.w = final_w;
        self.o2.h = final_h;
        self.o2.cells = new_cells;
        self.o2.locks = new_locks;
        self.o2.ports = new_ports;
        self.o2.port_names = new_port_names;

        self.select(
            self.cursor.cx as isize,
            self.cursor.cy as isize,
            self.cursor.cw,
            self.cursor.ch,
        );
        self.history.clear();
        self.history.record(&self.o2.cells);
        self.history.saved_absolute_index = None;
    }

    pub fn glyph_at(&self, x: usize, y: usize) -> char {
        if let Some(idx) = self.o2.index_at(x, y) {
            self.o2.cells[idx]
        } else {
            '.'
        }
    }

    pub fn is_locked(&self, x: usize, y: usize) -> bool {
        if let Some(idx) = self.o2.index_at(x, y) {
            self.o2.locks[idx]
        } else {
            false
        }
    }

    pub fn port_at(&self, x: usize, y: usize) -> Option<StyleType> {
        if let Some(idx) = self.o2.index_at(x, y) {
            self.o2.ports[idx]
        } else {
            None
        }
    }

    pub fn port_name_at(&self, x: usize, y: usize) -> Option<(&'static str, char)> {
        if let Some(idx) = self.o2.index_at(x, y) {
            self.o2.port_names[idx]
        } else {
            None
        }
    }

    pub fn operate(&mut self) {
        if self.bpm < self.bpm_target {
            self.bpm += 1;
        } else if self.bpm > self.bpm_target {
            self.bpm -= 1;
        }

        self.o2.tick(&mut self.midi.engine);

        if !self.o2.commands.is_empty() {
            let cmds: Vec<_> = self.o2.commands.drain(..).collect();
            for (msg, pos) in cmds {
                crate::app::commander::run_command(self, &msg, pos);
            }
        }
    }

    pub fn is_operator(g: char) -> bool {
        o2_rs::core::glyph::is_operator(g)
    }

    pub fn trigger(&mut self) {
        let g = self.glyph_at(self.cursor.cx, self.cursor.cy);
        if g != '.' && Self::is_operator(g) {
            o2_rs::core::operators::run(
                &mut self.o2,
                &mut self.midi.engine,
                self.cursor.cx,
                self.cursor.cy,
                g,
                true,
            );
            if !self.o2.commands.is_empty() {
                let cmds: Vec<_> = self.o2.commands.drain(..).collect();
                for (msg, pos) in cmds {
                    crate::app::commander::run_command(self, &msg, pos);
                }
            }
        }
    }

    pub fn copy(&mut self) {
        let mut s = String::new();
        for y in self.cursor.min_y..=self.cursor.max_y {
            for x in self.cursor.min_x..=self.cursor.max_x {
                s.push(self.glyph_at(x, y));
            }
            if y < self.cursor.max_y {
                s.push('\n');
            }
        }
        clipboard::copy(&s);
    }

    pub fn cut(&mut self) {
        self.copy();
        self.erase();
    }

    pub fn paste(&mut self) {
        if let Some(text) = clipboard::paste() {
            self.paste_text(&text);
        }
    }

    pub fn paste_text(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        let normalized = trimmed.replace("\r\n", "\n").replace('\r', "\n");
        let lines: Vec<&str> = normalized.split('\n').collect();

        for (j, line) in lines.iter().enumerate() {
            for (i, c) in line.chars().enumerate() {
                if self.mode == InputMode::Append && c == '.' {
                    continue;
                }
                self.o2
                    .write_silent(self.cursor.min_x + i, self.cursor.min_y + j, c);
            }
        }

        let w = lines[0].chars().count().saturating_sub(1) as isize;
        let h = lines.len().saturating_sub(1) as isize;

        self.select(self.cursor.min_x as isize, self.cursor.min_y as isize, w, h);
        self.history.record(&self.o2.cells);
    }

    #[cfg(not(target_os = "macos"))]
    pub fn get_midi_output_devices(&self) -> Vec<String> {
        let mut devices = Vec::new();
        if let Ok(midi_out) = midir::MidiOutput::new("o2") {
            for port in midi_out.ports() {
                if let Ok(name) = midi_out.port_name(&port) {
                    devices.push(name);
                }
            }
        }
        if self.midi.is_virtual_output {
            devices.push(self.midi.device_name.clone());
        }
        devices
    }

    #[cfg(target_os = "macos")]
    pub fn get_midi_output_devices(&self) -> Vec<String> {
        let mut devices: Vec<String> = (0..coremidi::Destinations::count())
            .filter_map(|i| coremidi::Destination::from_index(i).and_then(|d| d.display_name()))
            .collect();
        if self.midi.is_virtual_output {
            devices.push(self.midi.device_name.clone());
        }
        devices
    }

    pub fn set_midi_device(&mut self, name: &str) {
        self.midi.select_output_by_arg(name);
    }
}

impl std::fmt::Debug for EditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorState")
            .field("engine_w", &self.o2.w)
            .field("engine_h", &self.o2.h)
            .field("grid_w", &self.grid_w)
            .field("grid_h", &self.grid_h)
            .field("scroll_x", &self.scroll_x)
            .field("scroll_y", &self.scroll_y)
            .field("cx", &self.cursor.cx)
            .field("cy", &self.cursor.cy)
            .field("cw", &self.cursor.cw)
            .field("ch", &self.cursor.ch)
            .field("mode", &self.mode)
            .field("paused", &self.paused)
            .field("f", &self.o2.f)
            .field("bpm", &self.bpm)
            .field("bpm_target", &self.bpm_target)
            .field("last_input_was_mouse", &self.last_input_was_mouse)
            .field("midi_bclock", &self.midi_bclock)
            .field("midi", &self.midi)
            .finish_non_exhaustive()
    }
}
