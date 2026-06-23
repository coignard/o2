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
use crate::app::input::autocomplete_path;
use crate::app::types::{InputMode, PopupType, PromptPurpose};
use crate::app::ui::theme::{B_HIGH, B_INV, BG, F_HIGH, F_INV, F_LOW, F_MED, darken, style_colors};
use o2_rs::core::oxygen::StyleType;
use ratatui::{
    Frame,
    layout::{Constraint, HorizontalAlignment, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, Clear, List, ListItem, Paragraph, Row, Table},
};

fn is_marker(app: &EditorState, x: usize, y: usize) -> bool {
    x.is_multiple_of(app.grid_w) && y.is_multiple_of(app.grid_h)
}

fn is_near(app: &EditorState, x: usize, y: usize) -> bool {
    let left = (app.cursor.cx / app.grid_w) * app.grid_w;
    let right = left + app.grid_w;
    let top = (app.cursor.cy / app.grid_h) * app.grid_h;
    let bottom = top + app.grid_h;
    x >= left && x <= right && y >= top && y <= bottom
}

fn is_locals(app: &EditorState, x: usize, y: usize) -> bool {
    is_near(app, x, y)
        && x.is_multiple_of((app.grid_w / 4).max(1))
        && y.is_multiple_of((app.grid_h / 4).max(1))
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

fn apply_custom_colors(fg: Color, bg: Color, custom: &[Option<(u8, u8, u8)>; 3]) -> (Color, Color) {
    let remap = |c: Color| -> Color {
        if let Some((r, g, b)) = custom[0]
            && c == crate::app::ui::theme::B_LOW
        {
            Color::Rgb(r, g, b)
        } else if let Some((r, g, b)) = custom[1]
            && c == crate::app::ui::theme::B_MED
        {
            Color::Rgb(r, g, b)
        } else if let Some((r, g, b)) = custom[2]
            && c == crate::app::ui::theme::B_HIGH
        {
            Color::Rgb(r, g, b)
        } else {
            c
        }
    };
    (remap(fg), remap(bg))
}

fn resolve_colors(style: StyleType, bw: bool, contrast: bool) -> (Color, Color) {
    let (fg, bg) = style_colors(style);
    if bw {
        if bg.is_some() {
            (F_INV, F_HIGH)
        } else if fg.is_some() {
            (F_HIGH, BG)
        } else {
            (BG, BG)
        }
    } else if contrast && matches!(style, StyleType::Default | StyleType::Locked) {
        (F_HIGH, BG)
    } else {
        (fg.unwrap_or(crate::app::ui::theme::F_LOW), bg.unwrap_or(BG))
    }
}

fn write_ui(
    row: &mut [UiChar],
    text: &str,
    offset: usize,
    limit: usize,
    style: StyleType,
    bw: bool,
    contrast: bool,
) {
    let (fg, bg) = resolve_colors(style, bw, contrast);

    for (i, c) in text.chars().take(limit).enumerate() {
        if offset + i < row.len() {
            row[offset + i] = UiChar { c, fg, bg };
        }
    }
}

fn draw_grid(f: &mut Frame, app: &EditorState, area: Rect) {
    let scroll_x = app.scroll_x;
    let scroll_y = app.scroll_y;

    let visible_h = (area.height as usize).min(app.o2.h.saturating_sub(scroll_y));
    let visible_w = (area.width as usize).min(app.o2.w.saturating_sub(scroll_x));

    let mut lines = Vec::with_capacity(visible_h);
    let selection_glyph = app.glyph_at(app.cursor.cx, app.cursor.cy);

    for y in scroll_y..(scroll_y + visible_h) {
        let mut spans = Vec::new();
        let mut current_style = Style::new().bg(BG);
        let mut current_text = String::with_capacity(visible_w);

        for x in scroll_x..(scroll_x + visible_w) {
            let g = app.glyph_at(x, y);

            let (glyph, style) = if is_invisible(app, x, y, g) {
                (' ', Style::new().bg(BG))
            } else {
                let is_cursor = x == app.cursor.cx && y == app.cursor.cy;
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
                let (fg, bg) = resolve_colors(theme_type, app.bw, app.contrast);
                let (fg, bg) = apply_custom_colors(fg, bg, &app.custom_colors);
                let s = Style::new().fg(fg).bg(bg);

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

    f.render_widget(Paragraph::new(lines), area);
}

fn spans_to_line(row: &[UiChar]) -> Line<'_> {
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
}

fn draw_status_bar(f: &mut Frame, app: &EditorState, area: Rect) {
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

    let inspect = if app.cursor.cw != 0 || app.cursor.ch != 0 {
        "multi".to_string()
    } else if let Some((name, g)) = app.port_name_at(app.cursor.cx, app.cursor.cy) {
        if g == '.' {
            let mut chars = name.chars();
            match chars.next() {
                None => String::new(),
                Some(char_first) => char_first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        } else {
            format!("{}-{}", g, name)
        }
    } else if app.is_locked(app.cursor.cx, app.cursor.cy) {
        "locked".to_string()
    } else {
        "empty".to_string()
    };
    let mono = app.bw;
    let contrast = app.contrast;
    write_ui(
        &mut ui_l1,
        &inspect,
        0,
        gw - 1,
        StyleType::Input,
        mono,
        contrast,
    );

    let mode_char = match app.mode {
        InputMode::Normal => "",
        InputMode::Append => "+",
        InputMode::Selection => "'",
        InputMode::Slide => "~",
    };
    let cur_str = format!("{},{}{}", app.cursor.cx, app.cursor.cy, mode_char);
    let cur_style = match app.mode {
        InputMode::Normal => StyleType::Input,
        InputMode::Append => StyleType::Haste,
        InputMode::Selection => StyleType::Selected,
        InputMode::Slide => StyleType::Reader,
    };
    write_ui(&mut ui_l1, &cur_str, gw, gw, cur_style, mono, contrast);

    write_ui(
        &mut ui_l1,
        &format!("{}:{}", app.cursor.cw, app.cursor.ch),
        gw * 2,
        gw,
        StyleType::Input,
        mono,
        contrast,
    );
    write_ui(
        &mut ui_l1,
        &format!("{}f{}", app.o2.f, if app.paused { "~" } else { "" }),
        gw * 3,
        gw,
        StyleType::Input,
        mono,
        contrast,
    );

    let io_count = app.midi.engine.last_io_count;
    let io_str = "|".repeat(io_count.min(gw.saturating_sub(1)));
    let io_inspect = format!("{:.<1$}", io_str, gw.saturating_sub(1));
    write_ui(
        &mut ui_l1,
        &io_inspect,
        gw * 4,
        gw - 1,
        StyleType::Input,
        mono,
        contrast,
    );

    let io_in_msg = if app.o2.f < 250 {
        format!("< {}", app.midi.input_device_name)
    } else {
        String::new()
    };
    write_ui(
        &mut ui_l1,
        &io_in_msg,
        gw * 5,
        gw * 4,
        StyleType::Input,
        mono,
        contrast,
    );

    if app.commander.active {
        let cmd_str = format!(
            "{}{}",
            app.commander.query,
            if app.o2.f.is_multiple_of(2) { "_" } else { "" }
        );
        write_ui(
            &mut ui_l2,
            &cmd_str,
            0,
            gw * 4,
            StyleType::Input,
            mono,
            contrast,
        );
    } else {
        write_ui(
            &mut ui_l2,
            concat!("v", env!("CARGO_PKG_VERSION")),
            0,
            gw,
            StyleType::Input,
            mono,
            contrast,
        );
        write_ui(
            &mut ui_l2,
            &format!("{}x{}", app.o2.w, app.o2.h),
            gw,
            gw,
            StyleType::Input,
            mono,
            contrast,
        );
        write_ui(
            &mut ui_l2,
            &format!("{}/{}", app.grid_w, app.grid_h),
            gw * 2,
            gw,
            StyleType::Input,
            mono,
            contrast,
        );

        let diff = app.bpm_target as isize - app.bpm as isize;
        let bpm_offset = if diff.abs() > 5 {
            if diff > 0 {
                format!("+{}", diff)
            } else {
                format!("{}", diff)
            }
        } else {
            String::new()
        };
        let beat = if app.o2.f.is_multiple_of(4) && diff == 0 {
            "*"
        } else {
            ""
        };
        let clock_str = format!("{}{}{}", app.bpm, bpm_offset, beat);

        let clock_style = if app.midi.is_puppet() {
            StyleType::Output
        } else if app.midi_bclock {
            StyleType::Clock
        } else if app.paused {
            StyleType::Default
        } else {
            StyleType::Input
        };
        write_ui(
            &mut ui_l2,
            &clock_str,
            gw * 3,
            gw,
            clock_style,
            mono,
            contrast,
        );

        let vars: String = app
            .o2
            .variables
            .iter()
            .enumerate()
            .filter(|&(_, &opt)| opt.is_some())
            .map(|(i, _)| i as u8 as char)
            .collect();

        if !vars.is_empty() {
            let max = gw.saturating_sub(1);
            let disp = if vars.len() <= max {
                vars
            } else {
                let var_offset = app.o2.f % vars.len();
                let mut d = String::new();
                d.push_str(&vars[var_offset..]);
                d.push_str(&vars[..var_offset]);
                d.chars().take(max).collect()
            };
            write_ui(
                &mut ui_l2,
                &disp,
                gw * 4,
                max,
                StyleType::Input,
                mono,
                contrast,
            );
        }

        let io_out_msg = if app.o2.f < 250 {
            format!("> {}", app.midi.device_name)
        } else {
            String::new()
        };
        write_ui(
            &mut ui_l2,
            &io_out_msg,
            gw * 5,
            gw * 4,
            StyleType::Input,
            mono,
            contrast,
        );
    }

    for uc in ui_l1.iter_mut().chain(ui_l2.iter_mut()) {
        let (fg, bg) = apply_custom_colors(uc.fg, uc.bg, &app.custom_colors);
        uc.fg = fg;
        uc.bg = bg;
    }

    let status_lines = vec![spans_to_line(&ui_l1), spans_to_line(&ui_l2)];
    f.render_widget(Paragraph::new(status_lines), area);
}

pub fn draw(f: &mut Frame, app: &EditorState) {
    f.render_widget(Block::new().style(Style::new().bg(BG)), f.area());

    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(f.area());
    draw_grid(f, app, chunks[0]);
    if app.guide {
        draw_guide(f, app, chunks[0]);
    }
    draw_status_bar(f, app, chunks[1]);

    let mut prev_rect: Option<Rect> = None;
    for popup_type in &app.popup {
        let rect = get_popup_rect(f.area(), popup_type, prev_rect);
        draw_popup_content(f, app, popup_type, rect);
        prev_rect = Some(rect);
    }
}

pub fn get_popup_rect(area: Rect, popup_type: &PopupType, prev_rect: Option<Rect>) -> Rect {
    let (mut width, mut height) = match popup_type {
        PopupType::Controls => (57, 25),
        PopupType::Operators => {
            const N_OPS: u16 = 35;
            const COL_INNER_W: u16 = 35;
            let avail_rows = area.height.saturating_sub(2).max(1);
            let rows_per_col = N_OPS.min(avail_rows);
            let num_cols = N_OPS.div_ceil(rows_per_col);
            (num_cols * COL_INNER_W + 2, rows_per_col + 2)
        }
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

fn draw_guide(f: &mut Frame, app: &EditorState, area: Rect) {
    let operators: &[(char, &str)] = &[
        ('a', "Outputs sum of inputs"),
        ('b', "Outputs difference of inputs"),
        ('c', "Outputs modulo of frame"),
        ('d', "Bangs on modulo of frame"),
        ('e', "Moves eastward, or bangs"),
        ('f', "Bangs if inputs are equal"),
        ('g', "Writes operands with offset"),
        ('h', "Halts southward operand"),
        ('i', "Increments southward operand"),
        ('j', "Outputs northward operand"),
        ('k', "Reads multiple variables"),
        ('l', "Outputs smallest input"),
        ('m', "Outputs product of inputs"),
        ('n', "Moves Northward, or bangs"),
        ('o', "Reads operand with offset"),
        ('p', "Writes eastward operand"),
        ('q', "Reads operands with offset"),
        ('r', "Outputs random value"),
        ('s', "Moves southward, or bangs"),
        ('t', "Reads eastward operand"),
        ('u', "Bangs on Euclidean rhythm"),
        ('v', "Reads and writes variable"),
        ('w', "Moves westward, or bangs"),
        ('x', "Writes operand with offset"),
        ('y', "Outputs westward operand"),
        ('z', "Transitions operand to target"),
        ('*', "Bangs neighboring operands"),
        ('#', "Halts line"),
        ('$', "Sends ORCA command"),
        (':', "Sends MIDI note"),
        ('!', "Sends MIDI control change"),
        ('?', "Sends MIDI pitch bend"),
        ('%', "Sends MIDI monophonic note"),
        ('=', "Sends OSC message"),
        (';', "Sends UDP message"),
    ];

    let (glyph_style, desc_style) = if app.bw {
        (
            Style::new().bg(F_HIGH).fg(F_INV),
            Style::new().bg(BG).fg(F_HIGH),
        )
    } else {
        let glyph_fg = if app.contrast { F_INV } else { F_LOW };
        (
            Style::new().bg(B_HIGH).fg(glyph_fg),
            Style::new().bg(BG).fg(F_HIGH),
        )
    };

    let frame = (area.height as usize).saturating_sub(4).max(1);

    for (i, &(g, d)) in operators.iter().enumerate() {
        let col = i / frame;
        let row = i % frame;
        let screen_x = col * 32 + 2;
        let screen_y = row + 2;

        if screen_y >= area.height as usize {
            continue;
        }

        if screen_x < area.width as usize {
            f.render_widget(
                Paragraph::new(g.to_string()).style(glyph_style),
                Rect::new(area.x + screen_x as u16, area.y + screen_y as u16, 1, 1),
            );
        }

        let desc_x = screen_x + 2;
        if desc_x < area.width as usize {
            let desc_w = (area.width as usize - desc_x).min(d.len()) as u16;
            if desc_w > 0 {
                f.render_widget(
                    Paragraph::new(d.to_string()).style(desc_style),
                    Rect::new(area.x + desc_x as u16, area.y + screen_y as u16, desc_w, 1),
                );
            }
        }
    }
}

fn draw_controls_popup(f: &mut Frame, popup_style: Style, bold_style: Style, rect: Rect) {
    let _ = bold_style;
    let controls = [
        ("Ctrl+Q", "Quit"),
        ("Arrow Keys", "Move Cursor"),
        ("Ctrl+D or F1", "Open Main Menu"),
        ("Ctrl+K", "Toggle Commander"),
        ("0-9, A-Z, a-z,", "Insert Character"),
        ("! : % = ; ? # * _", ""),
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
        ("( ) - + [ ] { }", "Adjust Grid Size and Rulers"),
        ("< and >", "Adjust BPM"),
    ];

    let rows: Vec<Row> = controls
        .iter()
        .map(|&(k, v)| {
            Row::new(vec![
                Cell::from(Line::from(k).alignment(HorizontalAlignment::Right)).style(popup_style),
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

fn draw_operators_popup(f: &mut Frame, popup_style: Style, bold_style: Style, rect: Rect) {
    let operators = [
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
        ('$', "Sends ORCA command."),
        (':', "Sends MIDI note."),
        ('!', "Sends MIDI control change."),
        ('?', "Sends MIDI pitch bend."),
        ('%', "Sends MIDI monophonic note."),
        ('=', "Sends OSC message."),
        (';', "Sends UDP message."),
    ];

    let block = Block::bordered().title(" Operators ").style(popup_style);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let n = operators.len();
    let rows_per_col = inner.height as usize;
    let num_cols = n.div_ceil(rows_per_col).max(1);
    let col_w = inner.width / num_cols as u16;

    for col in 0..num_cols {
        let start = col * rows_per_col;
        if start >= n {
            break;
        }
        let end = (start + rows_per_col).min(n);

        let col_x = inner.x + col as u16 * col_w;
        let this_col_w = if col == num_cols - 1 {
            inner.width.saturating_sub(col as u16 * col_w)
        } else {
            col_w
        };

        if this_col_w == 0 {
            break;
        }

        let col_rect = Rect::new(col_x, inner.y, this_col_w, inner.height);
        let desc_w = this_col_w.saturating_sub(3);

        let rows: Vec<Row> = operators[start..end]
            .iter()
            .map(|&(g, d)| {
                Row::new(vec![
                    Cell::from(Span::styled(format!(" {}", g), bold_style)),
                    Cell::from(Span::styled(format!(" {}", d), popup_style)),
                ])
            })
            .collect();

        let table = Table::new(rows, [Constraint::Length(3), Constraint::Length(desc_w)])
            .style(popup_style);

        f.render_widget(table, col_rect);
    }
}

fn draw_about_popup(f: &mut Frame, popup_style: Style, rect: Rect, opened_at: &std::time::Instant) {
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

    for line in STICKMAN[current_frame.f_idx] {
        let line_str = format!("{}{}", pad_str, line);
        lines.push(
            Line::from(Span::styled(line_str, popup_style)).alignment(HorizontalAlignment::Left),
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
            Line::from(Span::styled(text, popup_style)).alignment(HorizontalAlignment::Center),
        );
    }

    let p = Paragraph::new(lines)
        .block(Block::bordered().style(popup_style))
        .style(popup_style);
    f.render_widget(p, rect);
}

fn draw_main_menu_popup(
    f: &mut Frame,
    popup_style: Style,
    bold_style: Style,
    rect: Rect,
    selected: usize,
) {
    let items = [
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
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            if s.is_empty() {
                ListItem::new("").style(popup_style)
            } else if i == selected {
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

fn draw_confirm_new_popup(
    f: &mut Frame,
    popup_style: Style,
    bold_style: Style,
    rect: Rect,
    selected: usize,
) {
    let items = ["Cancel", "Create New File"];
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            if i == selected {
                ListItem::new(format!(" > {}", s)).style(bold_style)
            } else {
                ListItem::new(format!("   {}", s)).style(popup_style)
            }
        })
        .collect();
    let list = List::new(list_items).block(Block::bordered().title(" Sure? ").style(popup_style));
    f.render_widget(list, rect);
}

fn draw_confirm_quit_popup(
    f: &mut Frame,
    popup_style: Style,
    bold_style: Style,
    rect: Rect,
    selected: usize,
    has_file: bool,
) {
    let items: &[&str] = if has_file {
        &["Save", "Save As...", "Yes, do as I say!", "Cancel"]
    } else {
        &["Save As...", "Yes, do as I say!", "Cancel"]
    };
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            if i == selected {
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

fn draw_autofit_popup(
    f: &mut Frame,
    popup_style: Style,
    bold_style: Style,
    rect: Rect,
    selected: usize,
) {
    let items = ["Nicely", "Tightly"];
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            if i == selected {
                ListItem::new(format!(" > {}", s)).style(bold_style)
            } else {
                ListItem::new(format!("   {}", s)).style(popup_style)
            }
        })
        .collect();
    let list =
        List::new(list_items).block(Block::bordered().title(" Auto-fit ").style(popup_style));
    f.render_widget(list, rect);
}

fn draw_clock_popup(
    f: &mut Frame,
    popup_style: Style,
    bold_style: Style,
    rect: Rect,
    selected: usize,
    midi_bclock: bool,
) {
    let mark = if midi_bclock { '*' } else { ' ' };
    let item_str = format!("[{}] Send MIDI Beat Clock", mark);
    let list_items = vec![if selected == 0 {
        ListItem::new(format!(" > {}", item_str)).style(bold_style)
    } else {
        ListItem::new(format!("   {}", item_str)).style(popup_style)
    }];
    let list = List::new(list_items).block(
        Block::bordered()
            .title(" Clock & Timing ")
            .style(popup_style),
    );
    f.render_widget(list, rect);
}

fn draw_midi_popup(
    f: &mut Frame,
    popup_style: Style,
    bold_style: Style,
    rect: Rect,
    selected: usize,
    devices: &[String],
    active_name: &str,
) {
    let list_items: Vec<ListItem> = if devices.is_empty() {
        vec![ListItem::new("  No devices found").style(popup_style)]
    } else {
        devices
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mark = if devices[i] == active_name { '*' } else { ' ' };
                let prefix = if i == selected { '>' } else { ' ' };
                let style = if i == selected {
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
                .title(" MIDI Device Selection ")
                .style(popup_style),
        )
        .style(popup_style);
    f.render_widget(list, rect);
}

fn draw_prompt_popup(
    f: &mut Frame,
    app: &EditorState,
    popup_style: Style,
    rect: Rect,
    purpose: &PromptPurpose,
    input: &str,
    cursor: usize,
) {
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

    let blink = app.o2.f.is_multiple_of(2);
    let (cursor_fg_a, cursor_bg_a, cursor_fg_b, cursor_bg_b) = if app.bw {
        (F_HIGH, F_INV, F_INV, F_HIGH)
    } else {
        (B_INV, BG, BG, B_INV)
    };
    let cursor_style = if blink {
        Style::new().fg(cursor_fg_a).bg(cursor_bg_a)
    } else {
        Style::new().fg(cursor_fg_b).bg(cursor_bg_b)
    };

    for (i, c) in input.chars().enumerate() {
        if i == cursor {
            spans.push(Span::styled(c.to_string(), cursor_style));
        } else {
            spans.push(Span::styled(c.to_string(), popup_style));
        }
    }

    let (ac_fg, ac_bg) = if app.bw {
        (F_INV, F_HIGH)
    } else {
        (darken(B_INV, 60), B_INV)
    };

    if cursor == input.chars().count() {
        if !autocomplete_str.is_empty() {
            let mut ac_chars = autocomplete_str.chars();
            let first_char = ac_chars.next().unwrap();
            let rest: String = ac_chars.collect();
            spans.push(Span::styled(first_char.to_string(), cursor_style));
            if !rest.is_empty() {
                spans.push(Span::styled(rest, Style::new().fg(ac_fg).bg(ac_bg)));
            }
        } else {
            spans.push(Span::styled(" ", cursor_style));
        }
    } else if !autocomplete_str.is_empty() {
        spans.push(Span::styled(
            autocomplete_str,
            Style::new().fg(ac_fg).bg(ac_bg),
        ));
    }

    let p = Paragraph::new(Line::from(spans))
        .block(Block::bordered().title(title).style(popup_style))
        .style(popup_style);
    f.render_widget(p, rect);
}

fn draw_msg_popup(f: &mut Frame, popup_style: Style, rect: Rect, title: &str, text: &str) {
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

fn draw_roflcopter_popup(f: &mut Frame, popup_style: Style, rect: Rect, frame_idx: usize) {
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

    let frame = if frame_idx.is_multiple_of(2) {
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

fn draw_popup_content(f: &mut Frame, app: &EditorState, popup_type: &PopupType, rect: Rect) {
    let (popup_style, bold_style) = if app.bw {
        let s = Style::new().bg(F_HIGH).fg(F_INV);
        (s, s.add_modifier(Modifier::BOLD))
    } else {
        let s = Style::new().bg(B_INV).fg(BG);
        (s, s.add_modifier(Modifier::BOLD))
    };
    f.render_widget(Clear, rect);

    match popup_type {
        PopupType::Controls => draw_controls_popup(f, popup_style, bold_style, rect),
        PopupType::Operators => draw_operators_popup(f, popup_style, bold_style, rect),
        PopupType::About { opened_at } => draw_about_popup(f, popup_style, rect, opened_at),
        PopupType::MainMenu { selected } => {
            draw_main_menu_popup(f, popup_style, bold_style, rect, *selected)
        }
        PopupType::ConfirmNew { selected } => {
            draw_confirm_new_popup(f, popup_style, bold_style, rect, *selected)
        }
        PopupType::ConfirmQuit { selected, has_file } => {
            draw_confirm_quit_popup(f, popup_style, bold_style, rect, *selected, *has_file)
        }
        PopupType::AutofitMenu { selected } => {
            draw_autofit_popup(f, popup_style, bold_style, rect, *selected)
        }
        PopupType::ClockMenu { selected } => {
            draw_clock_popup(f, popup_style, bold_style, rect, *selected, app.midi_bclock)
        }
        PopupType::MidiMenu { selected, devices } => draw_midi_popup(
            f,
            popup_style,
            bold_style,
            rect,
            *selected,
            devices,
            &app.midi.device_name,
        ),
        PopupType::Prompt {
            purpose,
            input,
            cursor,
        } => draw_prompt_popup(f, app, popup_style, rect, purpose, input, *cursor),
        PopupType::Msg { title, text } => draw_msg_popup(f, popup_style, rect, title, text),
        PopupType::RoflCopter => draw_roflcopter_popup(f, popup_style, rect, app.o2.f),
    }
}
