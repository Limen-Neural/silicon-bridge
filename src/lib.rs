//! # spikenaut-fpga
//!
//! SNN-to-FPGA deployment pipeline for the Spikenaut neuromorphic ecosystem.
//!
//! This crate provides:
//! - **Q8.8 fixed-point parameter export** from trained SNN weights to `.mem` hex files
//!   compatible with Vivado `$readmemh` synthesis
//! - **FPGA spike readback** over UART using the SiliconBridge v3.0 protocol
//! - **Vivado timing report parsing** for WNS/LUT utilization CI/CD gating
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
//! use spikenaut_fpga::{FpgaParameterExporter, q88_to_f32};
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
//! spikenaut-fpga = { version = "0.1", features = ["uart"] }
//! ```

mod fpga_export;
mod fpga_metrics;

#[cfg(feature = "uart")]
mod fpga_bridge;

// Re-export public API
pub use fpga_export::{
    FpgaParameterExporter,
    FpgaParameters,
    FpgaMetadata,
    format_q88_hex,
    q88_to_f32,
};

pub use fpga_metrics::FpgaMetrics;

#[cfg(feature = "uart")]
pub use fpga_bridge::{FpgaBridge, find_fpga_ports};
