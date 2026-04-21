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

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use o2_rs::core::app::App;

const GRIDS: &[(&str, &str)] = &[
    ("io", include_str!("../examples/benchmarks/io.o2")),
    ("logic", include_str!("../examples/benchmarks/logic.o2")),
    ("notes", include_str!("../examples/benchmarks/notes.o2")),
    (
        "families",
        include_str!("../examples/benchmarks/families.o2"),
    ),
    (
        "cardinals",
        include_str!("../examples/benchmarks/cardinals.o2"),
    ),
    ("tables", include_str!("../examples/benchmarks/tables.o2")),
    ("rw", include_str!("../examples/benchmarks/rw.o2")),
];

fn setup_app(grid_str: &str, w: usize, h: usize) -> App {
    let mut app = App::new(w, h, 42, 100);
    app.load(grid_str, None);
    app.resize(w, h);
    app
}

fn bench_time_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_time");
    let frames_to_test = [1, 10, 50, 100];

    for &(name, grid) in GRIDS.iter() {
        for &frames in &frames_to_test {
            group.bench_with_input(
                BenchmarkId::new(name, format!("{}f", frames)),
                &frames,
                |b, &f| {
                    b.iter_batched_ref(
                        || setup_app(grid, 64, 64),
                        |app| {
                            for _ in 0..f {
                                app.operate();
                                app.f += 1;
                            }
                        },
                        BatchSize::LargeInput,
                    )
                },
            );
        }
    }
    group.finish();
}

fn bench_space_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_space");
    let sizes_to_test = [64, 128, 256];

    for &(name, grid) in GRIDS.iter() {
        for &size in &sizes_to_test {
            group.bench_with_input(
                BenchmarkId::new(name, format!("{}x{}", size, size)),
                &size,
                |b, &s| {
                    b.iter_batched_ref(
                        || setup_app(grid, s, s),
                        |app| {
                            for _ in 0..10 {
                                app.operate();
                                app.f += 1;
                            }
                        },
                        BatchSize::LargeInput,
                    )
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_time_scaling, bench_space_scaling);
criterion_main!(benches);
