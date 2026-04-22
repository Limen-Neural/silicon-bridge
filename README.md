<p align="center">
  <img src="docs/logo.png" width="220" alt="Spikenaut">
</p>

<h1 align="center">silicon-bridge</h1>
<p align="center">SNN-to-FPGA deployment pipeline: Q8.8 parameter export, .mem generation, and UART spike readback</p>

<p align="center">
  <a href="https://crates.io/crates/silicon-bridge"><img src="https://img.shields.io/crates/v/silicon-bridge" alt="crates.io"></a>
  <a href="https://docs.rs/silicon-bridge"><img src="https://docs.rs/silicon-bridge/badge.svg" alt="docs.rs"></a>
  <img src="https://img.shields.io/badge/license-GPL--3.0-orange" alt="GPL-3.0">
</p>

---

The Rust-side bridge between trained SNN parameters and FPGA hardware. Exports
weights and thresholds as Q8.8 fixed-point `.mem` files for Vivado/Quartus
`$readmemh`, and provides an async UART bridge for sending stimuli and reading
back spike states at runtime.

## Features

- `FpgaParameterExporter` — serialize SNN weights/thresholds to Q8.8 `.mem` format
- `to_q88(value)` — convert `f32` to Q8.8 fixed-point (`value × 256 → u16`)
- `FpgaBridge` — async UART protocol for host-FPGA spike exchange
- `FpgaMetrics` — Vivado timing report parser (WNS, TNS, LUT utilization) for CI/CD gating
- Generic `FixedPoint<INT_BITS, FRAC_BITS>` type (Q8.8, Q4.12, Q12.4, …)
- Round-trip validation: export → program FPGA → read back → compare

## Installation

```toml
silicon-bridge = "0.1"
```

## Quick Start

### Export Parameters

```rust
use silicon_bridge::{FpgaParameterExporter, FpgaParameters};

let params = FpgaParameters {
    weights:    vec![0.5, -0.3, 0.8, /* ... */],
    thresholds: vec![0.6; 16],
    decay_rates: vec![0.9; 16],
};

let exporter = FpgaParameterExporter::new();
exporter.export_mem("weights.mem", &params)?;
// → weights.mem ready for Vivado $readmemh
```

### UART Spike Readback

```rust
use silicon_bridge::FpgaBridge;

let mut bridge = FpgaBridge::new()?;
let stimuli = vec![0.1; 16];
let (_potentials, spikes) = bridge.process_stimuli(&stimuli)?;
```

## Q8.8 Fixed-Point Format

```
Q8.8:  value = raw_u16 / 256.0
       raw   = round(value × 256)
Range: [0, 255.996]  (unsigned)
       [-128, 127.996]  (signed, two's complement)
```

Directly loadable by `WeightRam.sv` and `NeuronParamRam.sv`.

## Extracted from Production

Extracted from [Eagle-Lander](https://github.com/rmems/Eagle-Lander), a private
neuromorphic GPU supervisor. The FPGA export pipeline was decoupled from the private
training orchestrator so it works with any SNN framework.

## Related Ecosystem

| Library | Purpose |
|---------|---------|
| [silicon-bridge-sv](https://github.com/Limen-Neural/silicon-bridge-sv) | FPGA bridge and protocol layer |
| [silicon-distill-jl](https://github.com/Limen-Neural/silicon-distill-jl) | Julia training + distillation |

## License

GPL-3.0-or-later
