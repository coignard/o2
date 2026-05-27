<div align="center"><a href="https://github.com/coignard/o2">
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

<picture>
  <source srcset="https://github.com/coignard/o2/blob/main/assets/screenshot.png?raw=true">
  <img src="assets/screenshot.png" alt="O₂">
</picture>

</div>

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
o2-rs = "0.2.5"
```

## Extensions

O₂ extends the original ORCΛ operator set with one additional glyph.

The `_` character is valid in the length port of the MIDI (`:`) and Mono (` % `) operators. It creates a note with no scheduled Note Off.

## Test

```bash
cargo test
```

## Credits

O₂ is a Rust port of the [ORCΛ](https://github.com/hundredrabbits/Orca) esoteric programming language and livecoding environment, combining the best of the original JS and C implementations by [Hundred Rabbits](https://github.com/hundredrabbits) (Devine Lu Linvega & Rek Bell).

## License

The O₂ source code is © 2026 René Coignard and licensed under the [GNU General Public License v3.0 or later](LICENSE).

The `examples/` directory contains patch files from the [Orca-C](https://github.com/hundredrabbits/Orca-c) project, © 2017 Hundredrabbits, and are distributed under the [MIT License](examples/LICENSE).
