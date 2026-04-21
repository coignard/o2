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

//! Entry point and main event loop.
//!
//! [`main`] initialises the crossterm raw-mode terminal, creates an [`EditorState`],
//! and drives the event loop. The loop uses a phase-locked approach to clock
//! timing: a `next_clock_tick` instant is advanced by a fixed `clock_rate`
//! each iteration, eliminating timer drift that would otherwise cause rhythmic
//! jitter in MIDI output.

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use o2_rs::core::app::{EditorState, PopupType};
use o2_rs::{editor::input, ui::render};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Parser, Debug)]
#[command(
    name = "o2",
    version,
    override_usage = "o2 [options] [file]",
    disable_help_flag = true,
    disable_version_flag = true,
    help_template = "Usage: {usage}\n\n{all-args}"
)]
struct Cli {
    /// Set the maximum number of undo steps.
    /// If you plan to work with large files,
    /// set this to a low number.
    /// Default: 100
    #[arg(
        long,
        default_value_t = 100,
        hide_default_value = true,
        value_name = "number",
        help_heading = "General options",
        verbatim_doc_comment
    )]
    undo_limit: usize,

    /// When creating a new grid file, use these
    /// starting dimensions.
    #[arg(
        long,
        value_parser = parse_size,
        value_name = "nxn",
        help_heading = "General options",
        verbatim_doc_comment
    )]
    initial_size: Option<(usize, usize)>,

    /// Set the tempo (beats per minute).
    /// Default: 120
    #[arg(
        long,
        default_value_t = 120,
        hide_default_value = true,
        value_name = "number",
        help_heading = "General options",
        verbatim_doc_comment
    )]
    bpm: usize,

    /// Set the seed for the random function.
    /// Default: 1
    #[arg(
        long,
        default_value_t = 1,
        hide_default_value = true,
        value_name = "number",
        help_heading = "General options",
        verbatim_doc_comment
    )]
    seed: u64,

    /// Print this message and exit.
    #[arg(
        short = 'h',
        long = "help",
        action = clap::ArgAction::Help,
        help_heading = "General options"
    )]
    help: Option<bool>,

    /// Print version information and exit.
    #[arg(
        short = 'V',
        long = "version",
        action = clap::ArgAction::Version,
        help_heading = "General options"
    )]
    version: Option<bool>,

    ///
    /// Reduce the timing jitter of outgoing MIDI and OSC messages.
    /// Uses more CPU time.
    #[arg(long, help_heading = "OSC/MIDI options", verbatim_doc_comment)]
    strict_timing: bool,

    #[arg(value_name = "file", hide = true)]
    file: Option<PathBuf>,
}

fn parse_size(s: &str) -> Result<(usize, usize), String> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err("Expected format NxM (e.g. 57x25)".to_string());
    }
    let w = parts[0].parse().map_err(|_| "Invalid width")?;
    let h = parts[1].parse().map_err(|_| "Invalid height")?;
    Ok((w, h))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let size = terminal.size()?;
    let mut term_w = size.width.max(1) as usize;
    let mut term_h = (size.height.saturating_sub(2)).max(1) as usize;

    if let Some((w, h)) = cli.initial_size {
        term_w = w;
        term_h = h;
    }

    let mut app = EditorState::new(term_w, term_h, cli.seed, cli.undo_limit);
    app.set_bpm(cli.bpm);

    if let Some(path) = cli.file
        && let Ok(content) = std::fs::read_to_string(&path)
    {
        app.load(&content, Some(path));
        app.resize(term_w, term_h);
    }

    if app.paused {
        app.update_ports();
    }

    let mut next_clock_tick = Instant::now();
    let mut clock_counter = 0;

    let mut needs_draw = true;

    loop {
        if needs_draw {
            terminal.draw(|f| render::draw(f, &app))?;
            needs_draw = false;
        }

        let tick_rate = Duration::from_millis(if app.paused {
            100
        } else {
            60000 / app.bpm.max(1) as u64 / 4
        });

        let clock_rate = tick_rate / 6;

        let mut now = Instant::now();
        let mut timeout = next_clock_tick.saturating_duration_since(now);

        if cli.strict_timing && timeout > Duration::from_millis(2) {
            timeout -= Duration::from_millis(2);
        } else if cli.strict_timing {
            timeout = Duration::from_millis(0);
        }

        if event::poll(timeout)? {
            match event::read()? {
                Event::Resize(_cols, _rows) => {
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                Event::Mouse(mouse_event) => {
                    input::handle_mouse(&mut app, mouse_event);
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                Event::Key(key) => {
                    input::handle_key(&mut app, key);
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                Event::Paste(ref text) => {
                    input::handle_paste(&mut app, text);
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                _ => {}
            }
        }

        now = Instant::now();
        if cli.strict_timing {
            while now < next_clock_tick {
                std::hint::spin_loop();
                now = Instant::now();
            }
        }

        if now >= next_clock_tick {
            if clock_counter == 0 && !app.paused {
                app.operate();
                app.midi.run();
                app.engine.f += 1;
                needs_draw = true;
            }

            if app.midi_bclock
                && !app.paused
                && let Some(conn) = app.midi.out.as_mut()
            {
                let _ = conn.send(&[0xF8]);
            }

            clock_counter = (clock_counter + 1) % 6;
            next_clock_tick += clock_rate;

            // ant mill
            if now.duration_since(next_clock_tick) > clock_rate * 12 {
                next_clock_tick = now + clock_rate;
            }
        }

        if app
            .popup
            .iter()
            .any(|p| matches!(p, PopupType::About { .. }))
        {
            needs_draw = true;
        }

        if !app.running {
            app.midi.silence();
            app.midi.send_clock_stop();
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    Ok(())
}
