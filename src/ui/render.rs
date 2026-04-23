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

//! Terminal user interface rendering.
//!
//! This module implements the complete draw cycle for o2, covering:
//!
//! - The main grid, with glyphs, port decorations, selection highlights, and
//!   scroll offsets computed dynamically to follow the cursor.
//! - The two-row status bar below the grid, showing the inspector, cursor
//!   position, frame counter, MIDI activity, BPM clock, and variables.
//! - All overlay popups stacked on top of the grid, from the main menu to
//!   single-line prompts and informational cards.
//!
//! The primary entry point is [`draw`], which is called once per render tick
//! from the main loop.

#![allow(clippy::manual_is_multiple_of)]

use crate::core::app::{EditorState, InputMode, PopupType, PromptPurpose};
use crate::editor::input::autocomplete_path;
use crate::ui::theme::{B_INV, BG, F_MED, StyleType, darken};
use ratatui::{
    Frame,
    layout::{Constraint, HorizontalAlignment, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Clear, List, ListItem, Paragraph, Row, Table},
};

fn is_marker(app: &EditorState, x: usize, y: usize) -> bool {
    x % app.grid_w == 0 && y % app.grid_h == 0
}

fn is_near(app: &EditorState, x: usize, y: usize) -> bool {
    let left = (app.cx / app.grid_w) * app.grid_w;
    let right = left + app.grid_w;
    let top = (app.cy / app.grid_h) * app.grid_h;
    let bottom = top + app.grid_h;
    x >= left && x <= right && y >= top && y <= bottom
}

fn is_locals(app: &EditorState, x: usize, y: usize) -> bool {
    is_near(app, x, y) && (x * 4) % app.grid_w.max(1) == 0 && (y * 4) % app.grid_h.max(1) == 0
}

fn is_invisible(app: &EditorState, x: usize, y: usize, g: char) -> bool {
    g == '.'
        && !is_marker(app, x, y)
        && !app.is_selected(x, y)
        && !is_locals(app, x, y)
        && app.port_at(x, y).is_none()
        && !app.is_locked(x, y)
}

fn make_style(app: &EditorState, x: usize, y: usize, glyph: char, selection: char) -> StyleType {
    if app.is_selected(x, y) {
        return StyleType::Selected;
    }
    let is_locked = app.is_locked(x, y);
    if selection == glyph && !is_locked && selection != '.' {
        return StyleType::Reader;
    }
    if glyph == '*' && !is_locked {
        return StyleType::Input;
    }
    if let Some(port_style) = app.port_at(x, y) {
        return port_style;
    }
    if is_locked {
        return StyleType::Locked;
    }
    StyleType::Default
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct UiChar {
    c: char,
    fg: Color,
    bg: Color,
}

fn write_ui(row: &mut [UiChar], text: &str, offset: usize, limit: usize, style: StyleType) {
    let (fg, bg) = style.colors();
    let fg = fg.unwrap_or(crate::ui::theme::F_LOW);
    let bg = bg.unwrap_or(BG);

    for (i, c) in text.chars().take(limit).enumerate() {
        if offset + i < row.len() {
            row[offset + i] = UiChar { c, fg, bg };
        }
    }
}

/// Renders the complete UI to the given ratatui [`Frame`].
///
/// The terminal area is divided into two vertical sections:
///
/// 1. **Grid area** -- the scrollable ORCΛ grid, rendered as a [`Paragraph`]
///    of styled [`Span`]s.  Consecutive cells that share the same style are
///    merged into a single span for efficiency.
/// 2. **Status area** -- two fixed rows at the bottom of the screen.  The
///    upper row shows the cell inspector, cursor coordinates, selection size,
///    frame counter, MIDI I/O indicators, and the MIDI input device name.
///    The lower row shows either the commander prompt or the version, grid
///    dimensions, BPM clock, active variables, and the MIDI output device
///    name.
///
/// Popup overlays are drawn last, on top of everything else, using
/// [`draw_popup_content`].
pub fn draw(f: &mut Frame, app: &EditorState) {
    f.render_widget(Block::new().style(Style::new().bg(BG)), f.area());

    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(f.area());

    let grid_area = chunks[0];
    let status_area = chunks[1];

    let scroll_x = app.scroll_x;
    let scroll_y = app.scroll_y;

    let visible_h = (grid_area.height as usize).min(app.engine.h.saturating_sub(scroll_y));
    let visible_w = (grid_area.width as usize).min(app.engine.w.saturating_sub(scroll_x));

    let mut lines = Vec::with_capacity(visible_h);
    let selection_glyph = app.glyph_at(app.cx, app.cy);

    for y in scroll_y..(scroll_y + visible_h) {
        let mut spans = Vec::new();
        let mut current_style = Style::new().bg(BG);
        let mut current_text = String::with_capacity(visible_w);

        for x in scroll_x..(scroll_x + visible_w) {
            let g = app.glyph_at(x, y);

            let (glyph, style) = if is_invisible(app, x, y, g) {
                (' ', Style::new().bg(BG))
            } else {
                let is_cursor = x == app.cx && y == app.cy;
                let marker = is_marker(app, x, y);

                let display_glyph = if g != '.' {
                    g
                } else if is_cursor {
                    if app.paused { '~' } else { '@' }
                } else if marker {
                    '+'
                } else {
                    g
                };

                let theme_type = make_style(app, x, y, display_glyph, selection_glyph);
                let (fg, bg) = theme_type.colors();

                let mut s = Style::new().bg(bg.unwrap_or(BG));
                if let Some(c) = fg {
                    s = s.fg(c);
                }

                (display_glyph, s)
            };

            if x == scroll_x || style == current_style {
                current_text.push(glyph);
            } else {
                spans.push(Span::styled(current_text.clone(), current_style));
                current_text.clear();
                current_text.push(glyph);
            }
            current_style = style;
        }

        if !current_text.is_empty() {
            spans.push(Span::styled(current_text, current_style));
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), grid_area);

    let w = (f.area().width as usize).max(1);
    let mut ui_l1 = vec![
        UiChar {
            c: ' ',
            fg: F_MED,
            bg: BG
        };
        w
    ];
    let mut ui_l2 = vec![
        UiChar {
            c: ' ',
            fg: F_MED,
            bg: BG
        };
        w
    ];
    let gw = app.grid_w;

    let inspect = if app.cw != 0 || app.ch != 0 {
        "multi".to_string()
    } else if let Some((name, g)) = app.port_name_at(app.cx, app.cy) {
        if g == '.' {
            let mut chars = name.chars();
            match chars.next() {
                None => String::new(),
                Some(char_first) => char_first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        } else {
            format!("{}-{}", g, name)
        }
    } else if app.is_locked(app.cx, app.cy) {
        "locked".to_string()
    } else {
        "empty".to_string()
    };
    write_ui(&mut ui_l1, &inspect, 0, gw - 1, StyleType::Input);

    let mode_char = match app.mode {
        InputMode::Normal => "",
        InputMode::Append => "+",
        InputMode::Selection => "'",
        InputMode::Slide => "~",
    };
    let cur_str = format!("{},{}{}", app.cx, app.cy, mode_char);
    let cur_style = match app.mode {
        InputMode::Normal => StyleType::Input,
        InputMode::Append => StyleType::Haste,
        InputMode::Selection => StyleType::Selected,
        InputMode::Slide => StyleType::Reader,
    };
    write_ui(&mut ui_l1, &cur_str, gw, gw, cur_style);

    write_ui(
        &mut ui_l1,
        &format!("{}:{}", app.cw, app.ch),
        gw * 2,
        gw,
        StyleType::Input,
    );
    write_ui(
        &mut ui_l1,
        &format!("{}f{}", app.engine.f, if app.paused { "~" } else { "" }),
        gw * 3,
        gw,
        StyleType::Input,
    );

    let io_count = app.midi.stack.len()
        + app.midi.mono_stack.iter().flatten().count()
        + app.midi.cc_stack.len();

    let io_str = "|".repeat(io_count.min(gw.saturating_sub(1)));
    let io_inspect = format!("{:.<1$}", io_str, gw.saturating_sub(1));
    write_ui(&mut ui_l1, &io_inspect, gw * 4, gw - 1, StyleType::Input);

    let io_in_msg = if app.engine.f < 250 {
        format!("< {}", app.midi.input_device_name)
    } else {
        String::new()
    };

    let input_style = if app.midi.input_index == -1 {
        StyleType::Default
    } else {
        StyleType::Input
    };

    write_ui(&mut ui_l1, &io_in_msg, gw * 5, gw * 4, input_style);

    if app.commander_active {
        let cmd_str = format!(
            "{}{}",
            app.query,
            if app.engine.f % 2 == 0 { "_" } else { "" }
        );
        write_ui(&mut ui_l2, &cmd_str, 0, gw * 4, StyleType::Input);
    } else {
        write_ui(&mut ui_l2, "v0.1.0", 0, gw, StyleType::Input);
        write_ui(
            &mut ui_l2,
            &format!("{}x{}", app.engine.w, app.engine.h),
            gw,
            gw,
            StyleType::Input,
        );
        write_ui(
            &mut ui_l2,
            &format!("{}/{}", app.grid_w, app.grid_h),
            gw * 2,
            gw,
            StyleType::Input,
        );

        let diff = app.bpm_target as isize - app.bpm as isize;
        let offset = if diff.abs() > 5 {
            if diff > 0 {
                format!("+{}", diff)
            } else {
                format!("{}", diff)
            }
        } else {
            String::new()
        };
        let beat = if app.engine.f % 4 == 0 && diff == 0 {
            "*"
        } else {
            ""
        };
        let clock_str = format!("{}{}{}", app.bpm, offset, beat);

        let clock_style = if app.midi_bclock {
            StyleType::Clock
        } else if app.paused {
            StyleType::Default
        } else {
            StyleType::Input
        };
        write_ui(&mut ui_l2, &clock_str, gw * 3, gw, clock_style);

        let vars: String = app
            .engine
            .variables
            .iter()
            .enumerate()
            .filter(|&(_, &c)| c != '.')
            .map(|(i, _)| i as u8 as char)
            .collect();

        if !vars.is_empty() {
            let max = gw.saturating_sub(1);
            let disp = if vars.len() <= max {
                vars
            } else {
                let offset = app.engine.f % vars.len();
                let mut d = String::new();
                d.push_str(&vars[offset..]);
                d.push_str(&vars[..offset]);
                d.chars().take(max).collect()
            };
            write_ui(&mut ui_l2, &disp, gw * 4, max, StyleType::Input);
        }

        let io_out_msg = if app.engine.f < 250 {
            format!("> {}", app.midi.device_name)
        } else {
            String::new()
        };
        write_ui(&mut ui_l2, &io_out_msg, gw * 5, gw * 4, StyleType::Input);
    }

    let to_line = |row: &[UiChar]| -> Line {
        let mut spans = Vec::new();
        let mut current_style = Style::new();
        let mut current_text = String::with_capacity(row.len());

        for (i, uc) in row.iter().enumerate() {
            let style = Style::new().fg(uc.fg).bg(uc.bg);
            if i == 0 || style == current_style {
                current_text.push(uc.c);
            } else {
                spans.push(Span::styled(current_text.clone(), current_style));
                current_text.clear();
                current_text.push(uc.c);
            }
            current_style = style;
        }
        if !current_text.is_empty() {
            spans.push(Span::styled(current_text, current_style));
        }
        Line::from(spans)
    };

    let status_lines = vec![to_line(&ui_l1), to_line(&ui_l2)];
    f.render_widget(Paragraph::new(status_lines), status_area);

    let mut prev_rect: Option<Rect> = None;
    for popup_type in &app.popup {
        let rect = get_popup_rect(f.area(), popup_type, prev_rect);
        draw_popup_content(f, app, popup_type, rect);
        prev_rect = Some(rect);
    }
}

/// Computes the bounding rectangle for a popup overlay.
///
/// Most informational popups (`Controls`, `Operators`, `About`, `Msg`) are
/// always centred regardless of any previously rendered popup.  Menu and
/// dialogue popups cascade to the right of the previous popup when there is
/// room, then drop below it, and finally fall back to the top-left corner.
///
/// The returned rectangle is clamped so it never extends beyond the terminal
/// area.
pub fn get_popup_rect(area: Rect, popup_type: &PopupType, prev_rect: Option<Rect>) -> Rect {
    let (mut width, mut height) = match popup_type {
        PopupType::Controls => (57, 25),
        PopupType::Operators => (38, 35),
        PopupType::About { .. } => (47, 13),
        PopupType::MainMenu { .. } => (26, 20),
        PopupType::MidiMenu { devices, .. } => {
            let mut max_len = 28;
            for d in devices {
                max_len = max_len.max(d.chars().count() as u16 + 14);
            }
            (max_len + 4, devices.len().max(1) as u16 + 2)
        }
        PopupType::ConfirmNew { .. } => (22, 4),
        PopupType::ConfirmQuit { has_file, .. } => (26, if *has_file { 6 } else { 5 }),
        PopupType::AutofitMenu { .. } => (15, 4),
        PopupType::ClockMenu { .. } => (30, 3),
        PopupType::Prompt { .. } => (40, 3),
        PopupType::Msg { title, text } => {
            let max_line_len = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
            let w = max_line_len.max(title.chars().count()).max(10) as u16 + 4;
            let h = text.lines().count() as u16 + 2;
            (w, h)
        }
        PopupType::RoflCopter => (40, 13),
    };

    width = width.min(area.width);
    height = height.min(area.height);

    let center_always = matches!(
        popup_type,
        PopupType::Controls
            | PopupType::Operators
            | PopupType::About { .. }
            | PopupType::Msg { .. }
            | PopupType::RoflCopter
    );

    let mut rect = match prev_rect {
        Some(prev) if !center_always => {
            let mut r = Rect::new(prev.x + prev.width, prev.y, width, height);
            if r.x + r.width > area.width {
                r.x = prev.x;
                r.y = prev.y + prev.height;
            }
            if r.x + r.width > area.width || r.y + r.height > area.height {
                r.x = 0;
                r.y = 0;
            }
            r
        }
        _ => {
            let vertical_margin = area.height.saturating_sub(height) / 2;
            let horizontal_margin = area.width.saturating_sub(width) / 2;

            let layout_y = Layout::vertical([
                Constraint::Length(vertical_margin),
                Constraint::Length(height.min(area.height)),
                Constraint::Min(0),
            ])
            .split(area);

            Layout::horizontal([
                Constraint::Length(horizontal_margin),
                Constraint::Length(width.min(area.width)),
                Constraint::Min(0),
            ])
            .split(layout_y[1])[1]
        }
    };

    rect.width = rect.width.min(area.width.saturating_sub(rect.x));
    rect.height = rect.height.min(area.height.saturating_sub(rect.y));
    rect
}

/// Renders the content of a single popup overlay into `rect`.
///
/// Each [`PopupType`] variant is drawn with an amber background and black
/// foreground (the `b_inv` / `f_inv` theme slots).  The widget kind varies:
/// tables for reference cards, lists for menus, and paragraphs for text
/// prompts and messages.
fn draw_popup_content(f: &mut Frame, app: &EditorState, popup_type: &PopupType, rect: Rect) {
    let bg_color = crate::ui::theme::B_INV;
    let fg_color = crate::ui::theme::BG;
    let popup_style = Style::new().bg(bg_color).fg(fg_color);
    let bold_style = popup_style.add_modifier(Modifier::BOLD);

    f.render_widget(Clear, rect);

    match popup_type {
        PopupType::Controls => {
            let controls = vec![
                ("Ctrl+Q", "Quit"),
                ("Arrow Keys", "Move Cursor"),
                ("Ctrl+D or F1", "Open Main Menu"),
                ("Ctrl+K", "Toggle Commander"),
                ("0-9, A-Z, a-z,", "Insert Character"),
                ("! : % / = # *", ""),
                ("Spacebar", "Play/Pause"),
                ("Ctrl+Z or Ctrl+U", "Undo"),
                ("Ctrl+X", "Cut"),
                ("Ctrl+C", "Copy"),
                ("Ctrl+V", "Paste"),
                ("Ctrl+S", "Save"),
                ("Ctrl+F", "Frame Step Forward"),
                ("Ctrl+R", "Reset Frame Number"),
                ("Ctrl+I / Tab", "Append/Overwrite Mode"),
                ("' (quote)", "Rectangle Selection Mode"),
                ("Shift+Arrow Keys", "Adjust Rectangle Selection"),
                ("Alt+Arrow Keys", "Slide Selection"),
                ("` (grave) or ~", "Slide Selection Mode"),
                ("Escape", "Normal Mode/Deselect"),
                ("( ) _ + [ ] { }", "Adjust Grid Size and Rulers"),
                ("< and >", "Adjust BPM"),
                ("?", "Controls (this message)"),
            ];

            let rows: Vec<Row> = controls
                .into_iter()
                .map(|(k, v)| {
                    Row::new(vec![
                        Cell::from(Line::from(k).alignment(HorizontalAlignment::Right))
                            .style(popup_style),
                        Cell::from(Span::styled(
                            if v.is_empty() {
                                String::new()
                            } else {
                                format!("  {}", v)
                            },
                            popup_style,
                        )),
                    ])
                })
                .collect();

            let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(30)])
                .block(Block::bordered().title(" Controls ").style(popup_style))
                .style(popup_style);

            f.render_widget(table, rect);
        }

        PopupType::Operators => {
            let operators = vec![
                ('A', "Outputs sum of inputs."),
                ('B', "Outputs difference of inputs."),
                ('C', "Outputs modulo of frame."),
                ('D', "Bangs on modulo of frame."),
                ('E', "Moves eastward, or bangs."),
                ('F', "Bangs if inputs are equal."),
                ('G', "Writes operands with offset."),
                ('H', "Halts southward operand."),
                ('I', "Increments southward operand."),
                ('J', "Outputs northward operand."),
                ('K', "Reads multiple variables."),
                ('L', "Outputs smallest input."),
                ('M', "Outputs product of inputs."),
                ('N', "Moves Northward, or bangs."),
                ('O', "Reads operand with offset."),
                ('P', "Writes eastward operand."),
                ('Q', "Reads operands with offset."),
                ('R', "Outputs random value."),
                ('S', "Moves southward, or bangs."),
                ('T', "Reads eastward operand."),
                ('U', "Bangs on Euclidean rhythm."),
                ('V', "Reads and writes variable."),
                ('W', "Moves westward, or bangs."),
                ('X', "Writes operand with offset."),
                ('Y', "Outputs westward operand."),
                ('Z', "Transitions operand to target."),
                ('*', "Bangs neighboring operands."),
                ('#', "Halts line."),
                (':', "Sends MIDI note."),
                ('!', "Sends MIDI control change."),
                ('?', "Sends MIDI pitch bend."),
                ('=', "Sends OSC message."),
                (';', "Sends UDP message."),
            ];

            let rows: Vec<Row> = operators
                .into_iter()
                .map(|(g, d)| {
                    Row::new(vec![
                        Cell::from(Span::styled(format!(" {}", g), bold_style)),
                        Cell::from(Span::styled(format!(" {}", d), popup_style)),
                    ])
                })
                .collect();

            let table = Table::new(rows, [Constraint::Length(3), Constraint::Min(30)])
                .block(Block::bordered().title(" Operators ").style(popup_style))
                .style(popup_style);

            f.render_widget(table, rect);
        }

        PopupType::About { opened_at } => {
            const STICKMAN: [[&str; 3]; 11] = [
                ["   o   ", "  /|\\  ", "  / \\  "],
                [" \\ o / ", "   |   ", "  / \\  "],
                ["  _ o  ", "   /\\  ", "  | \\  "],
                ["       ", " ___\\o ", "/)   | "],
                ["__|   ", "   \\o  ", "   ( \\ "],
                ["  \\ /  ", "   |   ", "  /o\\  "],
                ["    |__", "  o/   ", "  / )  "],
                ["       ", "  o/__ ", "   | (\\"],
                ["  o _  ", "  /\\   ", "  / |  "],
                [" \\ o / ", "   |   ", "  / \\  "],
                ["   o   ", "  /|\\  ", "  / \\  "],
            ];

            struct AnimFrame {
                f_idx: usize,
                duration_ms: u64,
                x_pad: usize,
            }

            const TIMELINE: [AnimFrame; 11] = [
                AnimFrame {
                    f_idx: 0,
                    duration_ms: 750,
                    x_pad: 1,
                },
                AnimFrame {
                    f_idx: 1,
                    duration_ms: 750,
                    x_pad: 1,
                },
                AnimFrame {
                    f_idx: 2,
                    duration_ms: 250,
                    x_pad: 1,
                },
                AnimFrame {
                    f_idx: 3,
                    duration_ms: 150,
                    x_pad: 4,
                },
                AnimFrame {
                    f_idx: 4,
                    duration_ms: 100,
                    x_pad: 7,
                },
                AnimFrame {
                    f_idx: 5,
                    duration_ms: 100,
                    x_pad: 10,
                },
                AnimFrame {
                    f_idx: 6,
                    duration_ms: 100,
                    x_pad: 12,
                },
                AnimFrame {
                    f_idx: 7,
                    duration_ms: 200,
                    x_pad: 14,
                },
                AnimFrame {
                    f_idx: 8,
                    duration_ms: 250,
                    x_pad: 18,
                },
                AnimFrame {
                    f_idx: 9,
                    duration_ms: 1000,
                    x_pad: 18,
                },
                AnimFrame {
                    f_idx: 10,
                    duration_ms: u64::MAX,
                    x_pad: 18,
                },
            ];

            let elapsed = opened_at.elapsed().as_millis() as u64;

            let mut time_acc: u64 = 0;
            let mut current_frame = &TIMELINE[0];

            for frame in &TIMELINE {
                current_frame = frame;
                time_acc = time_acc.saturating_add(frame.duration_ms);
                if elapsed < time_acc {
                    break;
                }
            }

            let pad_str = " ".repeat(current_frame.x_pad);
            let mut lines = Vec::with_capacity(12);
            lines.push(Line::from(""));

            let frame = STICKMAN[current_frame.f_idx];

            for line in frame {
                let line_str = format!("{}{}", pad_str, line);
                lines.push(
                    Line::from(Span::styled(line_str, popup_style))
                        .alignment(HorizontalAlignment::Left),
                );
            }

            lines.push(Line::from(""));

            for &text in &[
                "Terminal Livecoding Environment",
                "",
                "(c) 2026 René Coignard",
                "(c) 2017-2026 Hundred Rabbits",
            ] {
                lines.push(
                    Line::from(Span::styled(text, popup_style))
                        .alignment(HorizontalAlignment::Center),
                );
            }

            let p = Paragraph::new(lines)
                .block(Block::bordered().style(popup_style))
                .style(popup_style);

            f.render_widget(p, rect);
        }

        PopupType::MainMenu { selected } => {
            let items = vec![
                "New",
                "Open...",
                "Save",
                "Save As...",
                "",
                "Set BPM...",
                "Set Grid Size...",
                "Auto-fit Grid",
                "",
                "MIDI Output...",
                "",
                "Clock & Timing...",
                "",
                "Controls...",
                "Operators...",
                "About o2...",
                "",
                "Quit",
            ];

            let list_items: Vec<ListItem> = items
                .into_iter()
                .enumerate()
                .map(|(i, s)| {
                    if s.is_empty() {
                        ListItem::new("").style(popup_style)
                    } else if i == *selected {
                        ListItem::new(format!(" > {}", s)).style(bold_style)
                    } else {
                        ListItem::new(format!("   {}", s)).style(popup_style)
                    }
                })
                .collect();

            let list = List::new(list_items)
                .block(Block::bordered().title(" o2 ").style(popup_style))
                .style(popup_style);

            f.render_widget(list, rect);
        }

        PopupType::ConfirmNew { selected } => {
            let items = ["Cancel", "Create New File"];
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == *selected {
                        ListItem::new(format!(" > {}", s)).style(bold_style)
                    } else {
                        ListItem::new(format!("   {}", s)).style(popup_style)
                    }
                })
                .collect();
            let list =
                List::new(list_items).block(Block::bordered().title(" Sure? ").style(popup_style));
            f.render_widget(list, rect);
        }

        PopupType::ConfirmQuit { selected, has_file } => {
            let items = if *has_file {
                vec!["Save", "Save As...", "Yes, do as I say!", "Cancel"]
            } else {
                vec!["Save As...", "Yes, do as I say!", "Cancel"]
            };
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == *selected {
                        ListItem::new(format!(" > {}", s)).style(bold_style)
                    } else {
                        ListItem::new(format!("   {}", s)).style(popup_style)
                    }
                })
                .collect();
            let list = List::new(list_items).block(
                Block::bordered()
                    .title(" Leaving so soon? ")
                    .style(popup_style),
            );
            f.render_widget(list, rect);
        }

        PopupType::AutofitMenu { selected } => {
            let items = ["Nicely", "Tightly"];
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == *selected {
                        ListItem::new(format!(" > {}", s)).style(bold_style)
                    } else {
                        ListItem::new(format!("   {}", s)).style(popup_style)
                    }
                })
                .collect();
            let list = List::new(list_items)
                .block(Block::bordered().title(" Auto-fit ").style(popup_style));
            f.render_widget(list, rect);
        }

        PopupType::ClockMenu { selected } => {
            let mark = if app.midi_bclock { '*' } else { ' ' };
            let items = [format!("[{}] Send MIDI Beat Clock", mark)];
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == *selected {
                        ListItem::new(format!(" > {}", s)).style(bold_style)
                    } else {
                        ListItem::new(format!("   {}", s)).style(popup_style)
                    }
                })
                .collect();
            let list = List::new(list_items).block(
                Block::bordered()
                    .title(" Clock & Timing ")
                    .style(popup_style),
            );
            f.render_widget(list, rect);
        }

        PopupType::MidiMenu { selected, devices } => {
            let active_idx = app.midi.output_index;
            let list_items: Vec<ListItem> = if devices.is_empty() {
                vec![ListItem::new("  No devices found").style(popup_style)]
            } else {
                devices
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let is_active = active_idx == i as i32;
                        let mark = if is_active { '*' } else { ' ' };
                        let prefix = if i == *selected { '>' } else { ' ' };
                        let style = if i == *selected {
                            bold_style
                        } else {
                            popup_style
                        };
                        ListItem::new(format!(" {} ({}) #{} - {}", prefix, mark, i, s)).style(style)
                    })
                    .collect()
            };

            let list = List::new(list_items)
                .block(
                    Block::bordered()
                        .title(" PortMidi Device Selection ")
                        .style(popup_style),
                )
                .style(popup_style);

            f.render_widget(list, rect);
        }

        PopupType::Prompt {
            purpose,
            input,
            cursor,
        } => {
            let title = match purpose {
                PromptPurpose::Open => " Open ",
                PromptPurpose::SaveAs { .. } => " Save As ",
                PromptPurpose::SetBpm => " Set BPM ",
                PromptPurpose::SetGridSize => " Set Grid Size ",
            };

            let autocomplete_str = match purpose {
                PromptPurpose::Open | PromptPurpose::SaveAs { .. } => {
                    autocomplete_path(input).unwrap_or_default()
                }
                _ => String::new(),
            };

            let mut spans = vec![Span::styled(" ", popup_style)];
            let mut chars = input.chars().peekable();
            let mut i = 0;

            let blink = app.engine.f % 2 == 0;
            let cursor_style = if blink {
                Style::new().fg(B_INV).bg(BG)
            } else {
                Style::new().fg(BG).bg(B_INV)
            };

            while let Some(c) = chars.next() {
                if i == *cursor {
                    spans.push(Span::styled(c.to_string(), cursor_style));
                } else {
                    spans.push(Span::styled(c.to_string(), popup_style));
                }
                i += 1;
            }

            let ac_color = darken(B_INV, 60);

            if *cursor == input.chars().count() {
                if !autocomplete_str.is_empty() {
                    let mut ac_chars = autocomplete_str.chars();
                    let first_char = ac_chars.next().unwrap();
                    let rest: String = ac_chars.collect();

                    spans.push(Span::styled(first_char.to_string(), cursor_style));
                    if !rest.is_empty() {
                        spans.push(Span::styled(rest, Style::new().fg(ac_color).bg(B_INV)));
                    }
                } else {
                    spans.push(Span::styled(" ", cursor_style));
                }
            } else if !autocomplete_str.is_empty() {
                spans.push(Span::styled(
                    autocomplete_str,
                    Style::new().fg(ac_color).bg(B_INV),
                ));
            }

            let p = Paragraph::new(Line::from(spans))
                .block(Block::bordered().title(title).style(popup_style))
                .style(popup_style);
            f.render_widget(p, rect);
        }

        PopupType::Msg { title, text } => {
            let lines: Vec<Line> = text
                .lines()
                .map(|l| Line::from(format!(" {}", l)))
                .collect();
            let p = Paragraph::new(lines)
                .block(
                    Block::bordered()
                        .title(format!(" {} ", title))
                        .style(popup_style),
                )
                .style(popup_style);
            f.render_widget(p, rect);
        }

        PopupType::RoflCopter => {
            const FRAME_0: &[&str] = &[
                "      ROFL:ROFL:LOL:              ",
                "           ______|____            ",
                "      LOL===        []\\          ",
                "            \\          \\        ",
                "             \\_________ ]        ",
                "                I   I             ",
                "             -------------/       ",
                "                                  ",
                "           ROFL COPTER!!!         ",
            ];

            const FRAME_1: &[&str] = &[
                "               :LOL:ROFL:ROFL     ",
                "       L   ______|____            ",
                "       O ===        []\\          ",
                "       L    \\          \\        ",
                "             \\_________ ]        ",
                "                I   I             ",
                "             -------------/       ",
                "                                  ",
                "           ROFL COPTER!!!         ",
            ];

            let frame = if app.engine.f % 2 == 0 {
                FRAME_0
            } else {
                FRAME_1
            };

            let mut lines = Vec::with_capacity(11);
            lines.push(Line::from(""));

            for &line in frame {
                lines.push(
                    Line::from(vec![
                        Span::styled(" ", popup_style),
                        Span::styled(line, popup_style),
                    ])
                    .alignment(HorizontalAlignment::Left),
                );
            }
            lines.push(Line::from(""));

            let p = Paragraph::new(lines)
                .block(Block::bordered().style(popup_style))
                .style(popup_style);

            f.render_widget(p, rect);
        }
    }
}
