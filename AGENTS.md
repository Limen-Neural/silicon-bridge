# AGENTS.md

Instructions for AI coding agents working on this repository.

## Project overview

`silicon-bridge` is a Rust crate for SNN-to-FPGA deployment. It exports trained spiking neural network parameters as Q8.8 fixed-point `.mem` files for Vivado synthesis, provides a UART bridge for runtime spike exchange, and parses Vivado timing reports for CI/CD gating.

- **License**: dual MIT / Apache-2.0
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

All 4 tests must pass. Add tests for any new code you write.

## Code style

- SPDX license header on every `.rs` file: `// SPDX-License-Identifier: MIT OR Apache-2.0`
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
- Keep PRs focused — one issue per PR when possible
- Link PR to the issue it addresses

## Security

- No secrets, API keys, DSNs, or credentials in code or commits
- No `unsafe` code without explicit justification
- UART feature is properly gated behind `#[cfg(feature = "uart")]`
