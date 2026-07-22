// SPDX-License-Identifier: MIT OR Apache-2.0
//! FPGA Synthesis Metrics — Vivado Report Parser
//!
//! Parses **WNS** from Vivado timing summary reports for CI gating.
//! LUT utilization is reserved on [`FpgaMetrics`] but not filled from reports yet.
//! Extracted from Eagle-Lander's SpikingInferenceEngine (engine.rs).

use serde::{Deserialize, Serialize};

/// FPGA synthesis and implementation metrics parsed from Vivado reports.
///
/// Parsed from `Basys3_Top_timing_summary_routed.rpt` in ship_ssn_logic/runs/impl_1/.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FpgaMetrics {
    /// Worst Negative Slack in nanoseconds.
    /// Negative value = timing violation. Positive = margin.
    pub wns_ns: f32,
    /// LUT resource utilization (0.0–1.0)
    pub lut_utilization: f32,
    /// `true` if the last synthesis/implementation run completed without errors
    pub synthesis_ok: bool,
}

impl FpgaMetrics {
    /// Parse the WNS from a Vivado timing summary report text.
    ///
    /// Looks for the `WNS(ns)` column header row and extracts the first value.
    /// Returns `None` if the file format is not recognized.
    pub fn parse_from_report(report_text: &str) -> Option<f32> {
        // The Vivado timing summary has a line like:
        // "  WNS(ns)      TNS(ns)  ..."
        // followed by a data row with the actual values.
        let mut found_header = false;
        for line in report_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("WNS(ns)") {
                found_header = true;
                continue;
            }
            if found_header && !trimmed.is_empty() {
                // First token of the data row is WNS
                if let Some(wns_str) = trimmed.split_whitespace().next() {
                    return wns_str.parse::<f32>().ok();
                }
                break;
            }
        }
        None
    }

    /// Attempt to load metrics from the canonical implementation report path.
    pub fn load_from_project() -> Option<Self> {
        let report_path =
            "fpga-project/ship_ssn_logic.runs/impl_1/Basys3_Top_timing_summary_routed.rpt";
        let text = std::fs::read_to_string(report_path).ok()?;
        let wns = Self::parse_from_report(&text)?;
        Some(Self {
            wns_ns: wns,
            lut_utilization: 0.0, // future enhancement
            synthesis_ok: true,
        })
    }

    /// Load metrics from a custom report path.
    pub fn load_from_path(report_path: &str) -> Option<Self> {
        let text = std::fs::read_to_string(report_path).ok()?;
        let wns = Self::parse_from_report(&text)?;
        Some(Self {
            wns_ns: wns,
            lut_utilization: 0.0,
            synthesis_ok: true,
        })
    }
}
