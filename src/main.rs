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
use o2_rs::{
    core::app::{EditorState, PopupType},
    editor::input,
    ui::render,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io::{self, Stdout},
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

    /// Reduce the timing jitter of outgoing MIDI and OSC messages.
    /// Uses more CPU time.
    #[arg(long, help_heading = "OSC/MIDI options", verbatim_doc_comment)]
    strict_timing: bool,

    /// Set MIDI to be sent via OSC formatted for Plogue Bidule.
    /// The path argument is the path of the Plogue OSC MIDI device.
    /// Example: /OSC_MIDI_0/MIDI
    #[arg(long, help_heading = "OSC/MIDI options", verbatim_doc_comment)]
    osc_midi_bidule: Option<String>,

    #[arg(value_name = "file", hide = true)]
    file: Option<PathBuf>,
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
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

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        crossterm::style::ResetColor,
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
        crossterm::cursor::Show
    );
}

fn emergency_save(app: &EditorState) {
    let save_path = if let Some(path) = &app.current_file {
        let mut os_string = path.as_os_str().to_os_string();
        os_string.push(".save");
        PathBuf::from(os_string)
    } else {
        PathBuf::from(format!("patch-{}.o2.save", input::arvelie_neralie()))
    };

    let mut content = String::with_capacity((app.engine.w + 1) * app.engine.h);
    for y in 0..app.engine.h {
        for x in 0..app.engine.w {
            content.push(app.engine.cells[y * app.engine.w + x]);
        }
        content.push('\n');
    }

    if std::fs::write(&save_path, content.trim_end()).is_ok() {
        eprintln!(
            "\n[o2] Application panicked! Emergency save created at: {}",
            save_path.display()
        );
    }
}

fn run_app(
    app: &mut EditorState,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    cli: &Cli,
) -> Result<()> {
    let mut next_clock_tick = Instant::now();
    let mut clock_counter = 0;
    let mut needs_draw = true;

    loop {
        if needs_draw {
            let size = terminal.size()?;
            let viewport_w = size.width as usize;
            let viewport_h = size.height.saturating_sub(2) as usize;
            app.update_scroll(viewport_w, viewport_h);

            terminal.draw(|f| render::draw(f, app))?;
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
                Event::Resize(cols, rows) => {
                    let new_w = (cols as usize).max(app.engine.w);
                    let new_h = (rows.saturating_sub(2) as usize).max(app.engine.h);
                    app.resize(new_w, new_h);
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                Event::Mouse(mouse_event) => {
                    input::handle_mouse(app, mouse_event);
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                Event::Key(key) => {
                    input::handle_key(app, key);
                    if app.paused {
                        app.update_ports();
                    }
                    needs_draw = true;
                }
                Event::Paste(ref text) => {
                    input::handle_paste(app, text);
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

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        crossterm::cursor::Hide
    )?;

    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let size = terminal.size()?;
    let mut term_w = size.width.max(1) as usize;
    let mut term_h = (size.height.saturating_sub(2)).max(1) as usize;

    if let Some((w, h)) = cli.initial_size {
        term_w = w;
        term_h = h;
    }

    let mut app = EditorState::new(term_w, term_h, cli.seed, cli.undo_limit);
    app.set_bpm(cli.bpm);
    app.midi.osc_midi_bidule = cli.osc_midi_bidule.clone();

    if let Some(path) = &cli.file
        && let Ok(content) = std::fs::read_to_string(path)
    {
        app.load(&content, Some(path.clone()));
        app.resize(term_w.max(app.engine.w), term_h.max(app.engine.h));
        app.history.saved_absolute_index = Some(app.history.offset + app.history.index);
    }

    if app.paused {
        app.update_ports();
    }

    let loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_app(&mut app, &mut terminal, &cli)
    }));

    match loop_result {
        Ok(result) => result,
        Err(err) => {
            emergency_save(&app);
            std::panic::resume_unwind(err);
        }
    }
}
