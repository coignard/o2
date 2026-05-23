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

#[cfg(target_os = "linux")]
use std::io::Write as _;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

/// Copies `text` to the system clipboard.
pub fn copy(text: &str) {
    if arboard::Clipboard::new()
        .and_then(|mut ctx| ctx.set_text(text))
        .is_err()
    {
        #[cfg(target_os = "linux")]
        {
            let _ = pipe_to("wl-copy", &[], text)
                || pipe_to("xclip", &["-selection", "clipboard"], text)
                || pipe_to("xsel", &["--clipboard", "--input"], text);
        }
    }
}

/// Returns the current clipboard text, or `None` if unavailable.
pub fn paste() -> Option<String> {
    if let Ok(mut ctx) = arboard::Clipboard::new()
        && let Ok(text) = ctx.get_text()
    {
        return Some(text);
    }
    #[cfg(target_os = "linux")]
    return read_from("wl-paste", &["--no-newline"])
        .or_else(|| read_from("xclip", &["-selection", "clipboard", "-out"]))
        .or_else(|| read_from("xsel", &["--clipboard", "--output"]));
    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(target_os = "linux")]
fn pipe_to(cmd: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn read_from(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}
