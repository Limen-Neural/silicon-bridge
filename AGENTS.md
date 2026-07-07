# AGENTS.md

Instructions for AI coding agents working on this repository.

## Identity

You are a Rust-focused coding agent. Write idiomatic Rust. Follow the conventions below for every change.

## Constraints (Do NOT)

- Do not commit secrets, API keys, DSNs, or credentials
- Do not add `unsafe` code without explicit safety justification
- Do not downgrade Rust edition from 2024 to 2021
- Do not add unused dependencies — check if a crate is actually imported before adding it
- Do not use relative links to license files in doc comments (they break on docs.rs)

## Tools

- `cargo check` — compile check
- `cargo test` — run unit tests and doctests
- `cargo build --release` — optimized build
- `cargo build --features uart` — build with UART bridge (requires `serialport`)
- `cargo clippy` — lint

## Project overview

`silicon-bridge` is a Rust project for SNN (Spiking Neural Network)-to-FPGA (Field-Programmable Gate Array) deployment.

- `fpga_export` exports trained SNN parameters as Q8.8 fixed-point `.mem` files for Vivado synthesis.
- `fpga_metrics` parses Vivado timing reports for CI/CD gating.
- `fpga_bridge` provides a UART bridge for runtime spike exchange (`uart` feature, `serialport`).

- **License**: dual MIT (Massachusetts Institute of Technology) / Apache-2.0
- **Rust edition**: 2024
- **Crate type**: library (`silicon_bridge`)

## Dev environment tips

```bash
cargo check                 # quick compile check
cargo test                  # run all tests + doctests
cargo build --release       # optimized build
cargo build --features uart # enable UART bridge (requires serialport)
```

Feature flags:

- `uart` — enables `FpgaBridge` and `find_fpga_ports` (requires `serialport` crate)

## Testing instructions

Tests live inline in source files:

| Module | Tests | What's covered |
|--------|-------|----------------|
| `src/fpga_export.rs` | 3 unit tests | Q8.8 conversion, parameter export, memory calculation |
| `src/lib.rs` | 1 doctest | Quick Start example |

Run before every commit:

```bash
cargo test
```

All tests must pass before pushing. Add tests for any new code you write.

## Code style

- SPDX (Software Package Data Exchange) license header on every `.rs` file: `// SPDX-License-Identifier: MIT OR Apache-2.0`
- Module-level doc comments with `//!`
- Public items get `///` doc comments
- Use `serde` derive for serializable types
- Use `f32` for public API parameters (consistent with existing API)
- Conventional Commits for messages: `type(scope): description`

  - Types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`
  - Scopes: `fpga-export`, `fpga-metrics`, `fpga-bridge`, `ci`, `docs`

## PR instructions

- Branch naming: `feat/`, `fix/`, `chore/`, `docs/` prefix
- Run `cargo test` and `cargo check` before pushing
- One issue per PR — split multi-issue work into separate PRs
- Link PR to the issue it addresses

## Security

- No secrets, API keys, DSNs, or credentials in code or commits
- No `unsafe` code without explicit justification
- UART feature is properly gated behind `#[cfg(feature = "uart")]`
