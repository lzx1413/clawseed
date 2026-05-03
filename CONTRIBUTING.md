# Contributing to ClawSeed

Thanks for your interest in contributing to ClawSeed! This document explains how to get started.

## Getting Started

```bash
git clone https://github.com/lzx1413/clawseed.git
cd clawseed
cargo build
cargo test
```

### Prerequisites

- Rust 1.87+ (edition 2024)
- For Android: NDK + Android SDK 36

## How to Contribute

### Reporting Bugs

Open a [GitHub issue](https://github.com/lzx1413/clawseed/issues) with:

- Steps to reproduce
- Expected vs. actual behavior
- Rust version (`rustc --version`), OS, and relevant config

### Suggesting Features

Open an issue with the `enhancement` label. Describe the use case and why existing functionality doesn't cover it.

### Submitting Code

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Ensure all checks pass:
   ```bash
   cargo fmt --check
   cargo clippy
   cargo test
   ```
5. Submit a pull request against `main`

## Code Guidelines

### Rust

- Follow existing code style — `cargo fmt` is the authority
- `cargo clippy` must pass with no warnings
- All public APIs need documentation
- Write tests for new functionality
- Keep crate dependencies minimal — don't add a crate for something the standard library handles

### Architecture Rules

- **Unidirectional dependencies**: `api ← agent ← tools/providers/memory ← gateway`. Never introduce reverse dependencies.
- **Trait-first**: new capabilities go behind a trait in `clawseed-api`. Implementations live in their respective crates.
- **No feature creep in core**: the agent loop should stay simple — receive, call LLM, dispatch tools, loop. Application-level concerns belong outside.

### Commit Messages

- Use imperative mood: "add feature" not "added feature"
- Keep the subject line under 72 characters
- Use conventional prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`

### Pull Requests

- Keep PRs focused — one logical change per PR
- Include a clear description of what changed and why
- Link related issues
- Add tests for bug fixes to prevent regressions

## Project Structure

| Directory | Contents |
|-----------|----------|
| `crates/clawseed-api` | Core trait definitions |
| `crates/clawseed-agent` | Agent loop, hooks, dispatch, security |
| `crates/clawseed-tools` | Built-in tool implementations |
| `crates/clawseed-providers` | LLM provider implementations |
| `crates/clawseed-memory` | SQLite + vector search memory backend |
| `crates/clawseed-config` | TOML configuration |
| `crates/clawseed-gateway` | HTTP/WebSocket server |
| `crates/clawseed` | CLI binary |
| `clients/android` | Android demo app (Kotlin + Compose) |
| `docs/` | Documentation (English + Chinese) |

## Adding a New Tool

1. Create your tool struct in `crates/clawseed-tools/src/`
2. Implement the `Tool` trait (`name`, `description`, `parameters_schema`, `execute`)
3. Register it in `all_tools()` with a config gate
4. Add tests
5. Document the tool in the README's built-in tools section

## Adding a New Provider

1. Implement `Provider` trait in `crates/clawseed-providers/src/`
2. Implement `ProviderFactory` trait with `name()`, `aliases()`, `create()`
3. Register the factory in the default registry
4. Add integration tests

## License

By contributing, you agree that your contributions will be dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE).
