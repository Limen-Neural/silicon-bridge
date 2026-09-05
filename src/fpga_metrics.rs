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
    /// Blank lines and dashed column-rule rows after the header are skipped.
    /// Returns `None` if the file format is not recognized.
    pub fn parse_from_report(report_text: &str) -> Option<f32> {
        // The Vivado timing summary has a line like:
        // "  WNS(ns)      TNS(ns)  ..."
        // followed by a dashed rule, then a data row with the actual values.
        let mut found_header = false;
        for line in report_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("WNS(ns)") {
                found_header = true;
                continue;
            }
            if found_header && !trimmed.is_empty() {
                // First token of the data row is WNS. Skip dashed separator rows
                // (e.g. "-------      -------") that Vivado prints under headers.
                if let Some(wns_str) = trimmed.split_whitespace().next() {
                    if wns_str.bytes().all(|b| b == b'-') {
                        continue;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Full Vivado timing-summary shape: header, dashed column-rule, then data.
    /// `skips_dashed_separator_row` remains the focused unit case for the skip.
    const TIMING_SUMMARY: &str = "\
------------------------------------------------------------------
| Tool Version : Vivado v2022.2 (64-bit)
| Design       : Basys3_Top
------------------------------------------------------------------

Design Timing Summary
---------------------

    WNS(ns)      TNS(ns)  TNS Failing Endpoints  TNS Total Endpoints
    -------      -------  ---------------------  -------------------
      2.345        0.000                      0                 1234
";

    /// Assert two `f32` values agree to within float round-trip noise.
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn parses_wns_from_timing_summary() {
        let wns = FpgaMetrics::parse_from_report(TIMING_SUMMARY).expect("WNS row not parsed");
        assert_close(wns, 2.345);
    }

    #[test]
    fn parses_negative_wns() {
        let report = "WNS(ns)      TNS(ns)\n  -0.427        -1.882\n";
        let wns = FpgaMetrics::parse_from_report(report).expect("negative WNS not parsed");
        assert_close(wns, -0.427);
    }

    #[test]
    fn skips_blank_lines_between_header_and_data() {
        let report = "    WNS(ns)      TNS(ns)\n\n   \n\n      1.500        0.000\n";
        let wns = FpgaMetrics::parse_from_report(report).expect("data row after blanks not parsed");
        assert_close(wns, 1.5);
    }

    #[test]
    fn tolerates_tabs_and_trailing_whitespace() {
        let report = "\t WNS(ns) \t TNS(ns)  \n\t   0.875 \t 0.000  \n";
        let wns = FpgaMetrics::parse_from_report(report).expect("tab-separated row not parsed");
        assert_close(wns, 0.875);
    }

    #[test]
    fn header_without_data_row_returns_none() {
        let report = "    WNS(ns)      TNS(ns)\n";
        assert!(FpgaMetrics::parse_from_report(report).is_none());
    }

    #[test]
    fn missing_header_returns_none() {
        let report = "Design Timing Summary\n      2.345        0.000\n";
        assert!(FpgaMetrics::parse_from_report(report).is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(FpgaMetrics::parse_from_report("").is_none());
        assert!(FpgaMetrics::parse_from_report("   \n\n\t\n").is_none());
    }

    #[test]
    fn garbage_input_returns_none() {
        assert!(FpgaMetrics::parse_from_report("not a vivado report at all").is_none());
    }

    #[test]
    fn non_numeric_first_token_returns_none() {
        let report = "    WNS(ns)      TNS(ns)\n        N/A          N/A\n";
        assert!(FpgaMetrics::parse_from_report(report).is_none());
    }

    /// Vivado prints a dashed rule under the column headers. That row must be
    /// skipped so the following numeric data row supplies WNS.
    #[test]
    fn skips_dashed_separator_row() {
        let report =
            "    WNS(ns)      TNS(ns)\n    -------      -------\n      2.345        0.000\n";
        let wns = FpgaMetrics::parse_from_report(report).expect("separator row blocked WNS parse");
        assert_close(wns, 2.345);
    }

    #[test]
    fn separator_row_then_eof_returns_none() {
        let report = "    WNS(ns)      TNS(ns)\n    -------      -------\n";
        assert!(FpgaMetrics::parse_from_report(report).is_none());
    }

    #[test]
    fn load_from_path_reads_report_file() {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(TIMING_SUMMARY.as_bytes()).expect("write");
        file.flush().expect("flush");
        let path = file.path().to_str().expect("utf-8 path");

        let metrics = FpgaMetrics::load_from_path(path).expect("metrics not loaded");
        assert_close(metrics.wns_ns, 2.345);
        assert_close(metrics.lut_utilization, 0.0);
        assert!(metrics.synthesis_ok);
    }

    #[test]
    fn load_from_path_returns_none_for_missing_file() {
        assert!(FpgaMetrics::load_from_path("does/not/exist.rpt").is_none());
    }

    #[test]
    fn load_from_path_returns_none_for_unparsable_report() {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(b"no timing summary here\n").expect("write");
        file.flush().expect("flush");
        let path = file.path().to_str().expect("utf-8 path");

        assert!(FpgaMetrics::load_from_path(path).is_none());
    }
}
