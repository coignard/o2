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

use assert_json_diff::assert_json_eq;
use o2_rs::core::midi::MidiEngine;
use o2_rs::core::oxygen::OxygenEngine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use test_generator::test_resources;

#[derive(Deserialize)]
struct Case {
    grid: String,
    frames: Vec<usize>,
}

#[derive(Serialize)]
struct Snapshot {
    frame: usize,
    width: usize,
    height: usize,
    grid: Vec<String>,
}

fn snapshot_at(input: &str, frame: usize) -> Snapshot {
    let mut engine = OxygenEngine::new(1, 1, 42);
    engine.load_grid(input);
    let mut midi = MidiEngine::new();
    for _ in 0..frame {
        engine.tick(&mut midi);
        engine.f += 1;
        midi.run();
        midi.pending.clear();
    }

    let grid: Vec<String> = (0..engine.h)
        .map(|y| {
            engine.cells[y * engine.w..(y + 1) * engine.w]
                .iter()
                .collect()
        })
        .collect();

    Snapshot {
        frame,
        width: engine.w,
        height: engine.h,
        grid,
    }
}

#[test_resources("tests/cases/*.json")]
fn grid_matches_snapshot(case_path: &str) {
    let raw = fs::read_to_string(case_path).unwrap_or_else(|_| panic!("read case {case_path}"));
    let case: Case =
        serde_json::from_str(&raw).unwrap_or_else(|_| panic!("parse case {case_path}"));
    let name = Path::new(case_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("case file stem");
    let input = fs::read_to_string(format!("tests/grids/{}", case.grid))
        .unwrap_or_else(|_| panic!("read grid {}", case.grid));

    let update = std::env::var("UPDATE_SNAPSHOTS").is_ok();
    for &frame in &case.frames {
        let snapshot = snapshot_at(&input, frame);
        let dump_path = format!("tests/dumps/{name}/frame_{frame}.json");

        if update {
            fs::create_dir_all(format!("tests/dumps/{name}")).expect("create dump dir");
            let json = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
            fs::write(&dump_path, json).expect("write dump");
        } else {
            let expected_raw = fs::read_to_string(&dump_path)
                .unwrap_or_else(|_| panic!("missing dump {dump_path}; run UPDATE_SNAPSHOTS=1"));
            let expected: serde_json::Value =
                serde_json::from_str(&expected_raw).expect("parse expected dump");
            let actual = serde_json::to_value(&snapshot).expect("serialize snapshot");
            assert_json_eq!(actual, expected);
        }
    }
}
