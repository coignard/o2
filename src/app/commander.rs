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

use crate::app::editor::EditorState;

fn parse_command(cmd: &str) -> (String, String) {
    let mut parts = cmd.splitn(2, ':');
    let command = parts.next().unwrap_or("").trim().to_lowercase();
    let value = parts.next().unwrap_or("").trim().to_string();
    (command, value)
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

pub fn run_command(app: &mut EditorState, cmd: &str, origin: Option<(usize, usize)>) {
    app.guide = false;
    let (command, value) = parse_command(cmd);
    let value = value.as_str();

    match command.as_str() {
        "bpm" | "bp" => {
            if let Ok(v) = value.parse::<usize>() {
                app.set_bpm(v);
            }
        }
        "apm" | "ap" => {
            if let Ok(v) = value.parse::<usize>() {
                app.set_bpm_target(v);
            }
        }
        "frame" | "fr" => {
            if let Ok(v) = value.parse::<usize>() {
                app.o2.f = v;
            }
        }
        "play" | "pl" => {
            app.paused = false;
            app.midi.send_clock_start();
        }
        "stop" | "st" => {
            app.paused = true;
            app.midi.silence();
            app.midi.send_clock_stop();
        }
        "run" | "ru" => {
            app.operate();
            app.midi.flush();
            app.o2.f += 1;
        }
        "rewind" | "re" => {
            if let Ok(v) = value.parse::<usize>() {
                app.o2.f = app.o2.f.saturating_sub(v);
            }
        }
        "skip" | "sk" => {
            if let Ok(v) = value.parse::<usize>() {
                app.o2.f += v;
            }
        }
        "find" | "fi" => {
            let cells_str: String = app.o2.cells.iter().collect();
            if let Some(idx) = cells_str.find(value) {
                let x = idx % app.o2.w;
                let y = idx / app.o2.w;
                app.select(
                    x as isize,
                    y as isize,
                    value.chars().count().saturating_sub(1) as isize,
                    0,
                );
            }
        }
        "select" | "se" => {
            let p: Vec<&str> = value.split(';').collect();
            if p.len() >= 2
                && let (Ok(x), Ok(y)) = (p[0].parse::<isize>(), p[1].parse::<isize>())
            {
                let w = p.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
                let h = p.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
                app.select(x, y, w, h);
            }
        }
        "write" | "wr" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty() {
                let text = p[0];
                let x = p
                    .get(1)
                    .and_then(|v| v.parse::<isize>().ok())
                    .unwrap_or_else(|| {
                        origin
                            .map(|o| o.0 as isize)
                            .unwrap_or(app.cursor.cx as isize)
                    });
                let y = p
                    .get(2)
                    .and_then(|v| v.parse::<isize>().ok())
                    .unwrap_or_else(|| {
                        origin
                            .map(|o| o.1 as isize)
                            .unwrap_or(app.cursor.cy as isize)
                    });
                for (i, c) in text.chars().enumerate() {
                    let target_x = x + i as isize;
                    if target_x >= 0 && y >= 0 {
                        app.o2.write_silent(target_x as usize, y as usize, c);
                    }
                }
                app.history.record(&app.o2.cells);
            }
        }
        "time" | "ti" => {
            let ms = (15000u64 * app.o2.f as u64) / app.bpm.max(1) as u64;
            let total_seconds = ms / 1000;
            let minutes = (total_seconds / 60) % 60;
            let seconds = total_seconds % 60;
            let text = format!("{:02}{:02}", minutes, seconds);

            let x = origin
                .map(|o| o.0 as isize)
                .unwrap_or(app.cursor.cx as isize);
            let y = origin
                .map(|o| o.1 as isize)
                .unwrap_or(app.cursor.cy as isize);

            for (i, c) in text.chars().enumerate() {
                let target_x = x + i as isize;
                if target_x >= 0 && y >= 0 {
                    app.o2.write_silent(target_x as usize, y as usize, c);
                }
            }
            app.history.record(&app.o2.cells);
        }
        "cc" => {
            if let Ok(v) = value.parse::<u8>() {
                app.midi.engine.cc_offset = v;
            }
        }
        "pg" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty() {
                let channel = p[0].parse::<u8>().unwrap_or(0).min(15);
                let bank = p.get(1).and_then(|v| v.parse::<u8>().ok());
                let sub = p.get(2).and_then(|v| v.parse::<u8>().ok());
                let pgm = p.get(3).and_then(|v| v.parse::<u8>().ok());
                app.midi.send_pg(channel, bank, sub, pgm);
            }
        }
        "midi" | "mi" => {
            let p: Vec<&str> = value.split(';').collect();
            if let Some(out) = p.first().filter(|s| !s.is_empty()) {
                app.midi.select_output_by_arg(out);
            }
            if let Some(idx) = p.get(1).and_then(|s| s.parse::<i32>().ok()) {
                app.midi.select_input_by_index(idx);
            }
        }
        "osc" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty()
                && let Ok(v) = p[0].parse::<u16>()
            {
                app.midi.osc_port = v;
            }
        }
        "udp" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty()
                && let Ok(v) = p[0].parse::<u16>()
            {
                app.midi.udp_port = v;
            }
        }
        "ip" if !value.is_empty() => {
            app.midi.ip = value.to_string();
        }
        "copy" | "co" => app.copy(),
        "paste" | "pa" => app.paste(),
        "erase" | "er" => app.erase(),
        "inject" | "in" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty() {
                let filename = p[0];
                let x = p
                    .get(1)
                    .and_then(|v| v.parse::<isize>().ok())
                    .unwrap_or_else(|| {
                        origin
                            .map(|o| o.0 as isize)
                            .unwrap_or(app.cursor.cx as isize)
                    });
                let y = p
                    .get(2)
                    .and_then(|v| v.parse::<isize>().ok())
                    .unwrap_or_else(|| {
                        origin
                            .map(|o| o.1 as isize)
                            .unwrap_or(app.cursor.cy as isize)
                    });
                let base = std::path::Path::new(filename);
                let with_o2 = base.with_extension("o2");
                let with_orca = base.with_extension("orca");
                let mut candidates = vec![base.to_path_buf(), with_o2.clone(), with_orca.clone()];
                if let Some(dir) = app.current_file.as_deref().and_then(|f| f.parent()) {
                    candidates.push(dir.join(base));
                    candidates.push(dir.join(&with_o2));
                    candidates.push(dir.join(&with_orca));
                }
                if let Some(content) = candidates
                    .iter()
                    .find_map(|p| std::fs::read_to_string(p).ok())
                {
                    for (row, line) in content.lines().enumerate() {
                        for (col, c) in line.chars().enumerate() {
                            let tx = x + col as isize;
                            let ty = y + row as isize;
                            if tx >= 0 && ty >= 0 {
                                app.o2.write_silent(tx as usize, ty as usize, c);
                            }
                        }
                    }
                    app.cursor.cw = 0;
                    app.cursor.ch = 0;
                    app.history.record(&app.o2.cells);
                }
            }
        }
        "color" | "cl" => {
            let parts: Vec<&str> = value.split(';').collect();
            for (i, part) in parts.iter().enumerate().take(3) {
                if !part.is_empty() {
                    app.custom_colors[i] = parse_hex_color(part);
                }
            }
        }
        _ => {}
    }
}

pub fn preview_command(app: &mut EditorState) {
    let query = app.commander.query.clone();
    let (command, value) = parse_command(&query);
    let value = value.as_str();

    if command == "find" || command == "fi" {
        let cells_str: String = app.o2.cells.iter().collect();
        if let Some(idx) = cells_str.find(value) {
            let x = idx % app.o2.w;
            let y = idx / app.o2.w;
            app.select(
                x as isize,
                y as isize,
                value.chars().count().saturating_sub(1) as isize,
                0,
            );
        }
    } else if command == "select" || command == "se" {
        let p: Vec<&str> = value.split(';').collect();
        if p.len() >= 2
            && let (Ok(x), Ok(y)) = (p[0].parse::<isize>(), p[1].parse::<isize>())
        {
            let w = p.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
            let h = p.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
            app.select(x, y, w, h);
        }
    }
}
