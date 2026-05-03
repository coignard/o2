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

//! Commander: text-command interpreter for controlling the application.
//!
//! Commands follow the format `name:value` (or `alias:value`) and are
//! dispatched by [`run_command`]. A subset of commands also have a "preview"
//! mode (see [`preview_command`]) that takes effect while the user is still
//! typing, without committing the action.
//!
//! Every command also has a two-letter shorthand alias.

use crate::core::app::EditorState;

/// Parses and executes a commander command string.
///
/// # Command format
///
/// ```text
/// command[:value[;value2;...]]
/// ```
///
/// Commands and their aliases:
///
/// | Command  | Alias | Effect                         |
/// |----------|-------|--------------------------------|
/// | `bpm`    | `bp`  | Set BPM immediately            |
/// | `apm`    | `ap`  | Set BPM target (animated)      |
/// | `frame`  | `fr`  | Jump to a specific frame       |
/// | `play`   | `pl`  | Start playback                 |
/// | `stop`   | `st`  | Stop playback and silence MIDI |
/// | `run`    | `ru`  | Advance one frame              |
/// | `rewind` | `re`  | Rewind by N frames             |
/// | `skip`   | `sk`  | Skip forward N frames          |
/// | `find`   | `fi`  | Move cursor to first match     |
/// | `select` | `se`  | Move cursor to `x;y;w;h`       |
/// | `write`  | `wr`  | Write text at position         |
/// | `time`   | `ti`  | Write elapsed time as `MMSS`   |
/// | `cc`     |  --   | Set CC knob offset             |
/// | `pg`     |  --   | Send Program Change            |
/// | `osc`    |  --   | Set OSC output port            |
/// | `udp`    |  --   | Set UDP output port            |
/// | `ip`     |  --   | Set destination IP address     |
/// | `copy`   | `co`  | Copy selection to clipboard    |
/// | `paste`  | `pa`  | Paste from clipboard           |
/// | `erase`  | `er`  | Erase selection                |
///
/// # Parameters
///
/// * `origin` -- optional grid position used as the write target for `write`
///   and `time` commands when no explicit coordinates are given.
pub fn run_command(app: &mut EditorState, cmd: &str, origin: Option<(usize, usize)>) {
    let mut parts = cmd.splitn(2, ':');
    let command = parts.next().unwrap_or("").trim().to_lowercase();
    let value = parts.next().unwrap_or("").trim();

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
                app.engine.f = v;
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
            app.midi.run();
            app.engine.f += 1;
        }
        "rewind" | "re" => {
            if let Ok(v) = value.parse::<usize>() {
                app.engine.f = app.engine.f.saturating_sub(v);
            }
        }
        "skip" | "sk" => {
            if let Ok(v) = value.parse::<usize>() {
                app.engine.f += v;
            }
        }
        "find" | "fi" => {
            let cells_str: String = app.engine.cells.iter().collect();
            if let Some(idx) = cells_str.find(value) {
                let x = idx % app.engine.w;
                let y = idx / app.engine.w;
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
                    .unwrap_or_else(|| origin.map(|o| o.0 as isize).unwrap_or(app.cx as isize));
                let y = p
                    .get(2)
                    .and_then(|v| v.parse::<isize>().ok())
                    .unwrap_or_else(|| origin.map(|o| o.1 as isize).unwrap_or(app.cy as isize));
                for (i, c) in text.chars().enumerate() {
                    let target_x = x + i as isize;
                    if target_x >= 0 && y >= 0 {
                        app.write_silent(target_x as usize, y as usize, c);
                    }
                }
                app.history.record(&app.engine.cells);
                app.update_ports();
            }
        }
        "time" | "ti" => {
            let ms = (15000u64 * app.engine.f as u64) / app.bpm.max(1) as u64;
            let total_seconds = ms / 1000;
            let minutes = (total_seconds / 60) % 60;
            let seconds = total_seconds % 60;
            let text = format!("{:02}{:02}", minutes, seconds);

            let x = origin.map(|o| o.0 as isize).unwrap_or(app.cx as isize);
            let y = origin.map(|o| o.1 as isize).unwrap_or(app.cy as isize);

            for (i, c) in text.chars().enumerate() {
                let target_x = x + i as isize;
                if target_x >= 0 && y >= 0 {
                    app.write_silent(target_x as usize, y as usize, c);
                }
            }
            app.history.record(&app.engine.cells);
            app.update_ports();
        }
        "cc" => {
            if let Ok(v) = value.parse::<u8>() {
                app.midi.cc_offset = v;
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
        "osc" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty()
                && let Ok(v) = p[0].parse::<u16>()
            {
                app.midi.osc.port = v;
            }
        }
        "udp" => {
            let p: Vec<&str> = value.split(';').collect();
            if !p.is_empty()
                && let Ok(v) = p[0].parse::<u16>()
            {
                app.midi.udp.port = v;
            }
        }
        "ip" if !value.is_empty() => {
            app.midi.ip = value.to_string();
        }
        "copy" | "co" => app.copy(),
        "paste" | "pa" => app.paste(),
        "erase" | "er" => app.erase(),
        _ => {}
    }
}

/// Evaluates the current [`EditorState::query`] string as a live preview without
/// committing any permanent state change.
///
/// Only `find` and `select` commands have a meaningful preview: they move the
/// cursor to the matching position while the user continues typing, so the
/// match is visible before Enter is pressed.
pub fn preview_command(app: &mut EditorState) {
    let cmd = &app.query;
    let mut parts = cmd.splitn(2, ':');
    let command = parts.next().unwrap_or("").trim().to_lowercase();
    let value = parts.next().unwrap_or("").trim();

    if command == "find" || command == "fi" {
        let cells_str: String = app.engine.cells.iter().collect();
        if let Some(idx) = cells_str.find(value) {
            let x = idx % app.engine.w;
            let y = idx / app.engine.w;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_app() -> EditorState {
        EditorState::new(10, 10, 1, 100)
    }

    #[test]
    fn test_run_command_bpm() {
        let mut app = create_app();
        run_command(&mut app, "bpm:140", None);
        assert_eq!(app.bpm, 140);
        assert_eq!(app.bpm_target, 140);

        run_command(&mut app, "bp:120", None);
        assert_eq!(app.bpm, 120);

        run_command(&mut app, "bpm:500", None);
        assert_eq!(app.bpm, 360);

        run_command(&mut app, "bpm:0", None);
        assert_eq!(app.bpm, 1);
    }

    #[test]
    fn test_run_command_apm() {
        let mut app = create_app();
        run_command(&mut app, "apm:150", None);
        assert_eq!(app.bpm, 120);
        assert_eq!(app.bpm_target, 150);

        run_command(&mut app, "ap:160", None);
        assert_eq!(app.bpm_target, 160);
    }

    #[test]
    fn test_run_command_frame() {
        let mut app = create_app();
        run_command(&mut app, "frame:100", None);
        assert_eq!(app.engine.f, 100);

        run_command(&mut app, "fr:50", None);
        assert_eq!(app.engine.f, 50);
    }

    #[test]
    fn test_run_command_play_stop() {
        let mut app = create_app();
        app.paused = true;

        run_command(&mut app, "play", None);
        assert!(!app.paused);

        run_command(&mut app, "stop", None);
        assert!(app.paused);

        run_command(&mut app, "pl", None);
        assert!(!app.paused);

        run_command(&mut app, "st", None);
        assert!(app.paused);
    }

    #[test]
    fn test_run_command_run() {
        let mut app = create_app();
        app.engine.f = 10;
        app.write_silent(1, 1, 'E');
        run_command(&mut app, "run", None);
        assert_eq!(app.engine.f, 11);
        assert_eq!(app.glyph_at(1, 1), '.');
        assert_eq!(app.glyph_at(2, 1), 'E');

        run_command(&mut app, "ru", None);
        assert_eq!(app.engine.f, 12);
        assert_eq!(app.glyph_at(2, 1), '.');
        assert_eq!(app.glyph_at(3, 1), 'E');
    }

    #[test]
    fn test_run_command_skip_rewind() {
        let mut app = create_app();
        app.engine.f = 10;

        run_command(&mut app, "skip:5", None);
        assert_eq!(app.engine.f, 15);

        run_command(&mut app, "sk:2", None);
        assert_eq!(app.engine.f, 17);

        run_command(&mut app, "rewind:10", None);
        assert_eq!(app.engine.f, 7);

        run_command(&mut app, "re:10", None);
        assert_eq!(app.engine.f, 0);

        run_command(&mut app, "re:100", None);
        assert_eq!(app.engine.f, 0);
    }

    #[test]
    fn test_run_command_select() {
        let mut app = create_app();
        run_command(&mut app, "select:2;3;4;5", None);
        assert_eq!(app.cx, 2);
        assert_eq!(app.cy, 3);
        assert_eq!(app.cw, 4);
        assert_eq!(app.ch, 5);

        run_command(&mut app, "se:1;1", None);
        assert_eq!(app.cx, 1);
        assert_eq!(app.cy, 1);
        assert_eq!(app.cw, 0);
        assert_eq!(app.ch, 0);
    }

    #[test]
    fn test_run_command_write() {
        let mut app = create_app();
        run_command(&mut app, "write:hallo;1;1", None);
        assert_eq!(app.glyph_at(1, 1), 'h');
        assert_eq!(app.glyph_at(2, 1), 'a');
        assert_eq!(app.glyph_at(3, 1), 'l');
        assert_eq!(app.glyph_at(4, 1), 'l');
        assert_eq!(app.glyph_at(5, 1), 'o');
        assert_eq!(app.glyph_at(6, 1), '.');

        run_command(&mut app, "wr:fuck", Some((0, 0)));
        assert_eq!(app.glyph_at(0, 0), 'f');
        assert_eq!(app.glyph_at(1, 0), 'u');
        assert_eq!(app.glyph_at(2, 0), 'c');
        assert_eq!(app.glyph_at(3, 0), 'k');

        run_command(&mut app, "wr:overflow;8;8", None);
        assert_eq!(app.glyph_at(8, 8), 'o');
        assert_eq!(app.glyph_at(9, 8), 'v');
        assert_eq!(app.glyph_at(10, 8), '.');
    }

    #[test]
    fn test_run_command_find() {
        let mut app = create_app();
        app.write_silent(3, 3, 'o');
        app.write_silent(4, 3, 'x');
        app.write_silent(5, 3, 'y');
        app.write_silent(6, 3, 'g');
        app.write_silent(7, 3, 'e');
        app.write_silent(8, 3, 'n');

        run_command(&mut app, "find:oxygen", None);
        assert_eq!(app.cx, 3);
        assert_eq!(app.cy, 3);
        assert_eq!(app.cw, 5);
        assert_eq!(app.ch, 0);

        app.cx = 0;
        app.cy = 0;
        app.cw = 0;

        run_command(&mut app, "fi:oxygen", None);
        assert_eq!(app.cx, 3);
        assert_eq!(app.cy, 3);
        assert_eq!(app.cw, 5);
        assert_eq!(app.ch, 0);
    }

    #[test]
    fn test_run_command_midi_config() {
        let mut app = create_app();
        run_command(&mut app, "cc:12", None);
        assert_eq!(app.midi.cc_offset, 12);

        run_command(&mut app, "udp:12345", None);
        assert_eq!(app.midi.udp.port, 12345);

        run_command(&mut app, "osc:9000", None);
        assert_eq!(app.midi.osc.port, 9000);

        run_command(&mut app, "ip:192.168.1.100", None);
        assert_eq!(app.midi.ip, "192.168.1.100");
    }

    #[test]
    fn test_preview_command() {
        let mut app = create_app();
        app.write_silent(5, 5, 'x');
        app.query = "find:x".to_string();

        preview_command(&mut app);
        assert_eq!(app.cx, 5);
        assert_eq!(app.cy, 5);
        assert_eq!(app.cw, 0);

        app.query = "se:2;2;1;1".to_string();
        preview_command(&mut app);
        assert_eq!(app.cx, 2);
        assert_eq!(app.cy, 2);
        assert_eq!(app.cw, 1);
        assert_eq!(app.ch, 1);
    }

    #[test]
    fn test_unknown_command() {
        let mut app = create_app();
        let old_f = app.engine.f;
        let old_bpm = app.bpm;
        run_command(&mut app, "fck_afd:2026", None);
        assert_eq!(app.engine.f, old_f);
        assert_eq!(app.bpm, old_bpm);
    }

    #[test]
    fn test_run_command_time() {
        let mut app = create_app();
        app.engine.f = 0;
        app.bpm = 120;

        run_command(&mut app, "time", Some((0, 0)));
        assert_eq!(app.glyph_at(0, 0), '0');
        assert_eq!(app.glyph_at(1, 0), '0');
        assert_eq!(app.glyph_at(2, 0), '0');
        assert_eq!(app.glyph_at(3, 0), '0');

        app.engine.f = 480;
        run_command(&mut app, "ti", Some((0, 1)));
        assert_eq!(app.glyph_at(0, 1), '0');
        assert_eq!(app.glyph_at(1, 1), '1');
        assert_eq!(app.glyph_at(2, 1), '0');
        assert_eq!(app.glyph_at(3, 1), '0');
    }

    #[test]
    fn test_run_command_invalid_format() {
        let mut app = create_app();
        let old_bpm = app.bpm;
        run_command(&mut app, "bpm:", None);
        assert_eq!(app.bpm, old_bpm);

        run_command(&mut app, "se:1", None);
        assert_eq!(app.cx, 0);
    }

    #[test]
    fn test_run_command_multiple_params() {
        let mut app = create_app();
        run_command(&mut app, "pg:10;20;30;127", None);
    }

    #[test]
    fn test_commander_garbage_input() {
        let mut app = create_app();
        let initial_bpm = app.bpm;

        run_command(&mut app, "", None);
        run_command(&mut app, ":::", None);
        run_command(&mut app, "bpm:abc", None);
        run_command(&mut app, "se:A;B;C;D", None);
        run_command(&mut app, "fr:-9999999999999999", None);
        run_command(&mut app, "sk:!@#$", None);

        assert_eq!(app.bpm, initial_bpm);
    }

    #[test]
    fn test_commander_write_out_of_bounds() {
        let mut app = create_app();
        run_command(&mut app, "write:hello;-100;-100", None);
        run_command(&mut app, "wr:hello;999;999", None);

        for &cell in &app.engine.cells {
            assert_eq!(cell, '.');
        }
    }

    #[test]
    fn test_commander_select_extreme_bounds() {
        let mut app = create_app();
        run_command(&mut app, "se:-500;-500;9999;9999", None);

        assert_eq!(app.cx, 0);
        assert_eq!(app.cy, 0);
        assert!(app.cw <= app.engine.w as isize);
        assert!(app.ch <= app.engine.h as isize);
        assert!(app.max_x < app.engine.w);
        assert!(app.max_y < app.engine.h);
    }

    #[test]
    fn test_commander_pg_missing_params() {
        let mut app = create_app();
        run_command(&mut app, "pg:255", None);
        run_command(&mut app, "pg:0;999", None);
        run_command(&mut app, "pg:15;10;20", None);
        run_command(&mut app, "pg:;;;", None);
    }

    #[test]
    fn test_preview_command_safe_fail() {
        let mut app = create_app();

        app.query = "se:999999999999999999999999999999999".to_string();
        preview_command(&mut app);

        app.query = "find:\\u{0000}".to_string();
        preview_command(&mut app);
    }
}
