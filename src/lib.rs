// SPDX-License-Identifier: MIT OR Apache-2.0
//! # silicon-bridge
//!
//! SNN-to-FPGA deployment pipeline for FPGA-backed neuromorphic hardware.
//!
//! This crate provides:
//! - **Q8.8 fixed-point parameter export** (`FixedPointEncode`, `ParameterExport`,
//!   `MemFileWriter`) for [silicon-hdl](https://github.com/Limen-Neural/silicon-hdl)
//!   `WeightRam` / `NeuronParamRam` via Vivado `$readmemh`
//! - **FPGA spike readback** over UART using the SiliconBridge v3.0 protocol
//! - **Vivado timing report parsing** for WNS-based CI/CD gating
//!
//! Licensed under either of MIT or Apache-2.0 at your option.
//!
//! ## Q8.8 conventions
//!
//! Q8.8 always means "value × 256 packed into a 16-bit word", but this crate
//! carries **two different signedness conventions** and they are not
//! interchangeable. Parameters baked into the bitstream are unsigned; host
//! stimuli pushed over UART are signed two's complement.
//!
//! | Aspect | Parameter export (`.mem`) | Host stimuli (UART TX/RX) |
//! |---|---|---|
//! | Encode with | [`FixedPointEncode::encode_q88`] / [`encode_q88_unsigned`] | `encode_q88_signed` (`uart` feature) |
//! | Decode with | [`q88_to_f32`] | `q88_signed_to_f32` (`uart` feature) |
//! | Raw type | `u16` (unsigned) | `i16` (two's complement) |
//! | Width | 16 bits — 8 integer + 8 fractional | 16 bits — 8 integer + 8 fractional |
//! | Scaling | `raw = value × 256`, truncated toward zero | `raw = value × 256`, truncated toward zero |
//! | Encoder input clamp | `[0.0, 255.99609375]` (scaled clamp `0..=65535`) | `[-127.99, 127.99]` |
//! | Encoder raw output | `0..=65535` | `-32765..=32765` (saturates inside the `i16` limits) |
//! | Decoder accepts | any `u16`: `0..=65535` → `0.0..=255.99609375` | any `i16`: `-32768..=32767` → `-128.0..=127.99609375` |
//! | Serialized as | ASCII hex text, one `{:04X}` word per line (`$readmemh`) | raw binary, big-endian (MSB first) |
//! | Consumed by | silicon-hdl `WeightRam` / `NeuronParamRam` | SiliconBridge v3.0 UART frame |
//! | Use it for | weights, thresholds, decay rates | host stimuli, RX membrane potentials |
//!
//! Consequences worth remembering:
//!
//! - The export path **cannot represent negative values**; anything below `0.0`
//!   clamps to raw `0`. Apply your own offset/bias convention before export if
//!   trained weights can be negative.
//! - The signed *encoder* saturates at raw `±32765` (`±127.99 × 256`, truncated),
//!   inside the `i16` limits, so both ends of the TX range are symmetric. The
//!   *decoder* is wider on purpose: an FPGA response of `0x8000` legitimately
//!   decodes to `-128.0` and `0x7FFF` to `127.99609375`.
//! - Both encoders truncate toward zero rather than rounding, and both map `NaN`
//!   to raw `0`.
//!
//! ## Provenance
//!
//! Extracted from Eagle-Lander, the author's own private neuromorphic GPU supervisor
//! repository (closed-source). The FPGA export pipeline deployed trained SNN parameters
//! to Basys3 hardware in production before being open-sourced as a standalone crate.
//!
//! ## Quick Start
//!
//! ```rust
//! use silicon_bridge::{FpgaParameterExporter, q88_to_f32};
//!
//! let mut exporter = FpgaParameterExporter::new();
//! exporter.set_thresholds(vec![1.0; 16]);
//! exporter.set_weights(vec![vec![0.5; 16]; 16]);
//! exporter.set_decay_rates(vec![0.85; 16]);
//!
//! let params = exporter.export();
//! println!("Memory usage: {:.2} KB", params.metadata.memory_usage_kb);
//! ```
//!
//! ## FPGA Bridge (requires `uart` feature)
//!
//! ```toml
//! [dependencies]
//! silicon-bridge = { version = "0.1", features = ["uart"] }
//! ```

mod fpga_export;
mod fpga_metrics;

#[cfg(feature = "uart")]
mod fpga_bridge;

// Re-export public API
pub use fpga_export::{
    EXPORT_FORMAT_VERSION, FixedPointEncode, FpgaMetadata, FpgaParameterExporter, FpgaParameters,
    MemFileWriter, ParameterExport, encode_q88_unsigned, format_q88_hex, q88_to_f32,
};

pub use fpga_metrics::FpgaMetrics;

#[cfg(feature = "uart")]
pub use fpga_bridge::{
    FpgaBridge, STIMULUS_Q88_MAX, STIMULUS_Q88_MIN, encode_q88_signed, find_fpga_ports,
    q88_signed_to_f32,
};
