<p align="center">
  <img src="docs/logo.png" width="220" alt="Spikenaut">
</p>

<h1 align="center">silicon-bridge</h1>
<p align="center">SNN-to-FPGA deployment pipeline: Q8.8 parameter export, .mem generation, and UART spike readback</p>

<p align="center">
  <a href="https://crates.io/crates/silicon-bridge"><img src="https://img.shields.io/crates/v/silicon-bridge" alt="crates.io"></a>
  <a href="https://docs.rs/silicon-bridge"><img src="https://docs.rs/silicon-bridge/badge.svg" alt="docs.rs"></a>
  <img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue" alt="MIT/Apache-2.0">
</p>

---

The Rust-side bridge between trained SNN parameters and FPGA hardware. Exports
weights and thresholds as Q8.8 fixed-point `.mem` files for Vivado/Quartus
`$readmemh`, and provides an optional **synchronous** UART bridge — built on the
blocking `serialport` crate and gated behind the `uart` feature — for sending
stimuli and reading back spike states at runtime.

## Features

- **Export traits** for hardware alignment with [silicon-hdl](https://github.com/Limen-Neural/silicon-hdl):
  - `FixedPointEncode` — `f32` → Q8.8 (`u16`)
  - `ParameterExport` — build the FPGA parameter bundle
  - `MemFileWriter` — write `$readmemh` `.mem` files
- `FpgaParameterExporter` — default implementation of those traits
- `format_q88_hex` / `q88_to_f32` — Q8.8 helpers
- `FpgaBridge` — blocking UART host protocol for host–FPGA spike exchange,
  backed by `serialport` (`uart` feature); no async runtime is involved
- `FpgaMetrics` — Vivado timing report parser: **WNS only** for CI/CD gating.
  TNS is not parsed, and `lut_utilization` is a reserved field that the loaders
  leave at `0.0` (both tracked by #21)

## Installation

```toml
silicon-bridge = "0.1"
```

## Quick Start

### Export Parameters

```rust
use silicon_bridge::{FpgaParameterExporter, ParameterExport};

let mut exporter = FpgaParameterExporter::new();
exporter.set_thresholds(vec![0.6; 16]);
exporter.set_weights(vec![vec![0.5; 16]; 16]);
exporter.set_decay_rates(vec![0.9; 16]);

let params = ParameterExport::export(&exporter);
// → params.thresholds, .weights, .decay_rates are Vec<u16> (Q8.8 format)
// → ready for silicon-hdl WeightRam / NeuronParamRam via Vivado $readmemh
```

### UART Spike Readback (requires the `uart` feature)

```toml
silicon-bridge = { version = "0.1", features = ["uart"] }
```

```rust
use silicon_bridge::FpgaBridge;

let mut bridge = FpgaBridge::new()?;
let stimuli = vec![0.1; 16];
let (_potentials, spikes) = bridge.process_stimuli(&stimuli)?;
```

`FpgaBridge` is synchronous. `process_stimuli` writes the request frame and then
blocks until the FPGA replies or the port's 100 ms read timeout elapses; nothing
in this API returns a `Future`, and no async executor is required.

## Q8.8 Fixed-Point Format

```
Q8.8:  value = raw_u16 / 256.0
       raw   = clamp(value × 256, 0, 65535) truncated to u16
Range: [0, 255.996]  (unsigned)
       [-128, 127.996]  (signed, two's complement)
```

Directly loadable by silicon-hdl `WeightRam.sv` and `NeuronParamRam.sv`
([Limen-Neural/silicon-hdl](https://github.com/Limen-Neural/silicon-hdl)).

## Vivado Timing Metrics

`FpgaMetrics` parses **WNS** (worst negative slack, in nanoseconds) out of a
Vivado timing summary report so CI can gate on a timing violation:

```rust
use silicon_bridge::FpgaMetrics;

if let Some(metrics) = FpgaMetrics::load_from_path("Basys3_Top_timing_summary_routed.rpt") {
    assert!(metrics.wns_ns >= 0.0, "timing violation");
}
```

WNS is the only value actually parsed. Not implemented yet:

- **TNS** — there is no `tns_ns` field and no TNS parser
- **LUT utilization** — `FpgaMetrics::lut_utilization` exists as a field, but
  `load_from_project` and `load_from_path` hard-code it to `0.0`

Both are tracked by #21; until that lands, WNS is the only enforceable gate.

## Repo boundaries

See [docs/boundary-matrix.md](docs/boundary-matrix.md) for what this crate owns
versus `neuromod`, `brainstem-daemon`, `limbic-critic`, and `silicon-hdl`.

## Extracted from Production

Extracted from [Eagle-Lander](https://github.com/rmems/Eagle-Lander), a private
neuromorphic GPU supervisor. The FPGA export pipeline was decoupled from the private
training orchestrator so it works with any SNN framework.

## Related Ecosystem

| Library | Purpose |
|---------|---------|
| [silicon-hdl](https://github.com/Limen-Neural/silicon-hdl) | SystemVerilog core, bridge, and SoC for Basys3 / Artix-7 |
| [SynapticDistill.jl](https://github.com/Limen-Neural/SynapticDistill.jl) | Julia training + distillation (Q8.8 export path) |
| [neuromod](https://github.com/Limen-Neural/neuromod) | SNN dynamics / core runtime traits |

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
