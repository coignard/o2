# Contributing to o2

## Development Setup

```bash
git clone https://github.com/coignard/o2
cd o2
cargo build
cargo test
```

## Guidelines

### Code Style

- Run `cargo fmt` before committing
- Fix all `cargo clippy` warnings
- Add tests for new features

### Commit Messages

```
type(scope): brief description

Detailed explanation (if needed)
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`

### Pull Requests

1. Fork the repository
2. Create feature branch (`git checkout -b feat/describe-your-feature`)
3. Commit changes
4. Push to branch
5. Open pull request

### Testing

```bash
cargo test           # Run all tests
cargo clippy         # Linting
```
