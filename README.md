<div align="center"><a href="https://lottie.rs/">
  <picture>
    <source srcset="https://github.com/coignard/o2/blob/main/assets/orca.png?raw=true">
    <img src="assets/orca.png" alt="Orca" width="150">
  </picture>
</a>

Rust port of the ORCΛ esoteric programming language and terminal livecoding environment

[![CI](https://github.com/coignard/o2/workflows/CI/badge.svg)](https://github.com/coignard/o2/actions)
[![CodeQL](https://github.com/coignard/o2/workflows/CodeQL/badge.svg)](https://github.com/coignard/o2/security/code-scanning)
[![Documentation](https://docs.rs/o2-rs/badge.svg)](https://docs.rs/o2-rs)
[![codecov](https://codecov.io/github/coignard/o2/graph/badge.svg?token=CQSZUALQ0A)](https://codecov.io/github/coignard/o2)
[![Crates.io](https://img.shields.io/crates/v/o2-rs.svg)](https://crates.io/crates/o2-rs)
[![License: GPL-3.0-or-later](https://img.shields.io/crates/l/o2-rs.svg)](LICENSE)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/coignard)

![Demo](assets/demo.gif)

</div>

The original ORCΛ ecosystem has two reference implementations: a [JavaScript/Electron desktop app](https://github.com/hundredrabbits/Orca) and a [C terminal livecoding environment](https://github.com/hundredrabbits/Orca-c). The JS version has a polished interface, but runs on a garbage-collected runtime that introduces timing jitter, a real problem when sending MIDI. The C version is fast and memory-efficient, but its ncurses UI is bare-bones and its clipboard support requires shelling out to `xclip`.

O₂ borrows the JS version's UX and builds it on the flat-buffer memory layout from Orca-C, in Rust. That means a phase-locked MIDI clock, zero heap allocations per frame, a [ratatui](https://github.com/ratatui/ratatui) UI with span-merging to cut terminal I/O, and native clipboard via arboard.

If you already write ORCΛ patches, O₂ runs them as-is.

Batteries included.

## Install

To download the source code, build the O₂ binary, and install it in `$HOME/.cargo/bin` in one go run:

```bash
cargo install --locked --git https://github.com/coignard/o2
```

Or install via Homebrew:

```bash
brew install coignard/tap/o2
```

Alternatively, you can manually download the source code and build the O₂ binary with:

```bash
git clone https://github.com/coignard/o2
cd o2
cargo build --release
sudo cp target/release/o2 /usr/local/bin/
```

## Install as library

Run the following Cargo command in your project directory:

```bash
cargo add o2-rs
```

Or add the following line to your `Cargo.toml`:

```toml
o2-rs = "0.1.1"
```

## Extensions

O₂ extends the original ORCΛ operator set with one additional glyph.

The `_` character is valid in the length port of the MIDI (`:`) and Mono (` % `) operators. It creates a note with no scheduled Note Off.

```
:01C._
```

An explicit Note Off event can be sent with:

```
:01C.0
```

## Test

```bash
cargo test
```

## Credits

O₂ is a Rust port of the [ORCΛ](https://github.com/hundredrabbits/Orca) esoteric programming language and livecoding environment, combining the best of the original JS and C implementations by [Hundred Rabbits](https://github.com/hundredrabbits) (Devine Lu Linvega & Rek Bell).

The ORCΛ language specification, operator semantics, and example patches are their work.

Relevant implementations:

| Language   | Repository                                                        |
|------------|-------------------------------------------------------------------|
| JavaScript | [hundredrabbits/Orca](https://github.com/hundredrabbits/Orca)     |
| C          | [hundredrabbits/Orca-c](https://github.com/hundredrabbits/Orca-c) |

Built with [ratatui](https://github.com/ratatui/ratatui).

## Sponsors

<a href="https://cloud9.sh/">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/cloud9-hq/assets/blob/main/logos/logo-dark.svg?raw=true">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/cloud9-hq/assets/blob/main/logos/logo.svg?raw=true">
    <img src="https://github.com/cloud9-hq/assets/blob/main/logos/logo.svg?raw=true" alt="Cloud9 Logo" height="38">
  </picture>
</a>

## License

The O₂ source code is © 2026 René Coignard and licensed under the [GNU General Public License v3.0 or later](LICENSE).

The `examples/` directory contains patch files from the [Orca-C](https://github.com/hundredrabbits/Orca-c) project, © 2017 Hundredrabbits, and are distributed under the [MIT License](examples/LICENSE).
