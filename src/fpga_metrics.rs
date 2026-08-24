// SPDX-License-Identifier: MIT OR Apache-2.0
//! FPGA Synthesis Metrics — Vivado Report Parser
//!
//! Parses **WNS** and **TNS** from Vivado timing summary reports
//! (`report_timing_summary`, e.g. `*_timing_summary_routed.rpt`) and **LUT
//! utilization** from Vivado utilization reports (`report_utilization`, e.g.
//! `*_utilization_placed.rpt`) for CI gating.
//!
//! Timing metrics are required — a report without a parsable `WNS(ns)` row
//! yields `None`. TNS and LUT utilization are optional: when a report omits
//! them the corresponding field stays at `0.0` instead of failing the parse.
//!
//! Extracted from Eagle-Lander's SpikingInferenceEngine (engine.rs).

use serde::{Deserialize, Serialize};

/// Canonical post-route timing summary path inside the Vivado project tree.
const PROJECT_TIMING_REPORT: &str =
    "fpga-project/ship_ssn_logic.runs/impl_1/Basys3_Top_timing_summary_routed.rpt";

/// Canonical post-place utilization report path inside the Vivado project tree.
const PROJECT_UTILIZATION_REPORT: &str =
    "fpga-project/ship_ssn_logic.runs/impl_1/Basys3_Top_utilization_placed.rpt";

/// FPGA synthesis and implementation metrics parsed from Vivado reports.
///
/// Timing fields come from `Basys3_Top_timing_summary_routed.rpt` and
/// [`lut_utilization`](FpgaMetrics::lut_utilization) from
/// `Basys3_Top_utilization_placed.rpt`, both in `ship_ssn_logic.runs/impl_1/`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FpgaMetrics {
    /// Worst Negative Slack in nanoseconds.
    /// Negative value = timing violation. Positive = margin.
    pub wns_ns: f32,
    /// Total Negative Slack in nanoseconds, summed over all failing endpoints.
    /// `0.0` when timing is met — and also when the report has no TNS column.
    pub tns_ns: f32,
    /// LUT resource utilization (0.0–1.0).
    /// `0.0` when no utilization report was available.
    pub lut_utilization: f32,
    /// `true` if the last synthesis/implementation run completed without errors
    pub synthesis_ok: bool,
}

impl FpgaMetrics {
    /// Parse the WNS from a Vivado timing summary report text.
    ///
    /// Looks for the `WNS(ns)` column header row and extracts the first value
    /// of the data row beneath it, skipping blank lines and the rule of dashes
    /// Vivado prints under the headers.
    /// Returns `None` if the file format is not recognized.
    ///
    /// ```
    /// use silicon_bridge::FpgaMetrics;
    ///
    /// let report = "\
    ///     WNS(ns)      TNS(ns)
    ///     -------      -------
    ///       2.345        0.000
    /// ";
    /// assert_eq!(FpgaMetrics::parse_from_report(report), Some(2.345));
    /// ```
    pub fn parse_from_report(report_text: &str) -> Option<f32> {
        Self::parse_timing_column(report_text, 0)
    }

    /// Parse the TNS from a Vivado timing summary report text.
    ///
    /// TNS is the second column of the same data row as WNS
    /// (`WNS(ns)  TNS(ns)  ...`). Returns `None` when the report has no such
    /// column, which callers should treat as "not reported" rather than as a
    /// failure.
    ///
    /// ```
    /// use silicon_bridge::FpgaMetrics;
    ///
    /// let report = "\
    ///     WNS(ns)      TNS(ns)
    ///     -------      -------
    ///      -0.427       -1.882
    /// ";
    /// assert_eq!(FpgaMetrics::parse_tns_from_report(report), Some(-1.882));
    /// ```
    pub fn parse_tns_from_report(report_text: &str) -> Option<f32> {
        Self::parse_timing_column(report_text, 1)
    }

    /// Parse LUT utilization (0.0–1.0) from a Vivado **utilization** report.
    ///
    /// The source is `report_utilization` output — canonically
    /// `<project>.runs/impl_1/<top>_utilization_placed.rpt` — *not* the timing
    /// summary, which carries no resource numbers. The value is the `Util%`
    /// cell of the `Slice LUTs` row of the *Slice Logic* table (`CLB LUTs` on
    /// UltraScale devices), converted from percent to a fraction. Nested rows
    /// such as `LUT as Logic` are ignored. Returns `None` when the text has no
    /// such row.
    ///
    /// ```
    /// use silicon_bridge::FpgaMetrics;
    ///
    /// let report = "\
    /// | Site Type   | Used | Fixed | Available | Util% |
    /// | Slice LUTs  | 3182 |     0 |     20800 | 15.30 |
    /// ";
    /// let lut = FpgaMetrics::parse_lut_utilization(report).unwrap();
    /// assert!((lut - 0.153).abs() < 1e-6);
    /// ```
    pub fn parse_lut_utilization(report_text: &str) -> Option<f32> {
        for line in report_text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('|') {
                continue;
            }
            let mut cells = trimmed
                .split('|')
                .map(str::trim)
                .filter(|cell| !cell.is_empty());
            let Some(site_type) = cells.next() else {
                continue;
            };
            // Vivado marks rows influenced by fixed cells with a trailing `*`.
            let site_type = site_type.trim_end_matches('*').trim_end();
            if !site_type.eq_ignore_ascii_case("Slice LUTs")
                && !site_type.eq_ignore_ascii_case("CLB LUTs")
            {
                continue;
            }
            // `Util%` is the last populated cell; the column count varies by
            // Vivado version (some emit an extra `Prohibited` column).
            let Some(util_percent) = cells.next_back() else {
                continue;
            };
            let Ok(percent) = util_percent.parse::<f32>() else {
                continue;
            };
            if percent.is_finite() && percent >= 0.0 {
                return Some(percent / 100.0);
            }
        }
        None
    }

    /// Attempt to load metrics from the canonical implementation report paths.
    ///
    /// Reads the post-route timing summary for WNS/TNS and the post-place
    /// utilization report for LUT utilization. Returns `None` only when the
    /// timing summary is missing or unparsable; a missing utilization report
    /// leaves `lut_utilization` at `0.0`.
    pub fn load_from_project() -> Option<Self> {
        Self::load_from_reports(PROJECT_TIMING_REPORT, PROJECT_UTILIZATION_REPORT)
    }

    /// Load metrics from a custom timing summary report path.
    ///
    /// WNS is required; TNS falls back to `0.0` when the report has no TNS
    /// column. LUT utilization is read from this same file only if it also
    /// contains a `report_utilization` table (concatenated reports) — use
    /// [`FpgaMetrics::load_from_reports`] to read it from its own file.
    pub fn load_from_path(report_path: &str) -> Option<Self> {
        let text = std::fs::read_to_string(report_path).ok()?;
        Self::from_report_texts(&text, None)
    }

    /// Load metrics from a timing summary report plus a utilization report.
    ///
    /// The utilization report is best-effort: when it is missing or has no LUT
    /// row, `lut_utilization` stays `0.0` and the timing metrics are still
    /// returned.
    pub fn load_from_reports(
        timing_report_path: &str,
        utilization_report_path: &str,
    ) -> Option<Self> {
        let timing_text = std::fs::read_to_string(timing_report_path).ok()?;
        let utilization_text = std::fs::read_to_string(utilization_report_path).ok();
        Self::from_report_texts(&timing_text, utilization_text.as_deref())
    }

    /// Build metrics from already-loaded report texts, degrading gracefully
    /// when the optional TNS column or LUT row is absent.
    fn from_report_texts(timing_text: &str, utilization_text: Option<&str>) -> Option<Self> {
        let wns_ns = Self::parse_from_report(timing_text)?;
        let tns_ns = Self::parse_tns_from_report(timing_text).unwrap_or(0.0);
        let lut_utilization = utilization_text
            .and_then(Self::parse_lut_utilization)
            .or_else(|| Self::parse_lut_utilization(timing_text))
            .unwrap_or(0.0);
        Some(Self {
            wns_ns,
            tns_ns,
            lut_utilization,
            synthesis_ok: true,
        })
    }

    /// Parse the `index`-th whitespace-separated column of the timing summary
    /// data row (0 = WNS, 1 = TNS).
    fn parse_timing_column(report_text: &str, index: usize) -> Option<f32> {
        let data_row = timing_summary_data_row(report_text)?;
        data_row.split_whitespace().nth(index)?.parse::<f32>().ok()
    }
}

/// Locate the data row of a Vivado timing summary.
///
/// Scans for the `WNS(ns)` column header, then returns the first line beneath
/// it that is neither blank nor a column rule.
fn timing_summary_data_row(report_text: &str) -> Option<&str> {
    let mut found_header = false;
    for line in report_text.lines() {
        let trimmed = line.trim();
        if !found_header {
            if trimmed.starts_with("WNS(ns)") {
                found_header = true;
            }
            continue;
        }
        if trimmed.is_empty() || is_column_rule(trimmed) {
            continue;
        }
        return Some(trimmed);
    }
    None
}

/// `true` if `trimmed` is a rule of dashes underlining the column headers
/// rather than a data row.
fn is_column_rule(trimmed: &str) -> bool {
    !trimmed.is_empty() && trimmed.chars().all(|c| c == '-' || c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Timing summary in the shape Vivado emits, column rule included.
    const TIMING_SUMMARY_WITH_RULE: &str = "\
------------------------------------------------------------------
| Tool Version : Vivado v2022.2 (64-bit)
| Design       : Basys3_Top
------------------------------------------------------------------

Design Timing Summary
---------------------

    WNS(ns)      TNS(ns)  TNS Failing Endpoints  TNS Total Endpoints
    -------      -------  ---------------------  -------------------
      2.345       -1.882                      3                 1234
";

    /// `report_utilization` excerpt (`*_utilization_placed.rpt`).
    const UTILIZATION_REPORT: &str = "\
1. Slice Logic
--------------

+----------------------------+------+-------+-----------+-------+
|          Site Type         | Used | Fixed | Available | Util% |
+----------------------------+------+-------+-----------+-------+
| Slice LUTs                 | 3182 |     0 |     20800 | 15.30 |
|   LUT as Logic             | 3050 |     0 |     20800 | 14.66 |
|   LUT as Memory            |  132 |     0 |      9600 |  1.38 |
| Slice Registers            | 1024 |     0 |     41600 |  2.46 |
+----------------------------+------+-------+-----------+-------+
";

    /// Assert two `f32` values agree to within float round-trip noise.
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    /// Write `contents` to a temp file and hand back the handle plus its path.
    fn report_file(contents: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(contents.as_bytes()).expect("write");
        file.flush().expect("flush");
        file
    }

    fn path_of(file: &tempfile::NamedTempFile) -> &str {
        file.path().to_str().expect("utf-8 path")
    }

    #[test]
    fn column_rule_is_skipped_before_the_data_row() {
        let wns =
            FpgaMetrics::parse_from_report(TIMING_SUMMARY_WITH_RULE).expect("WNS row not parsed");
        assert_close(wns, 2.345);
    }

    #[test]
    fn parses_tns_from_timing_summary() {
        let tns = FpgaMetrics::parse_tns_from_report(TIMING_SUMMARY_WITH_RULE)
            .expect("TNS column not parsed");
        assert_close(tns, -1.882);
    }

    #[test]
    fn parses_zero_tns_when_timing_is_met() {
        let report = "    WNS(ns)      TNS(ns)\n      2.345        0.000\n";
        assert_close(
            FpgaMetrics::parse_tns_from_report(report).expect("TNS not parsed"),
            0.0,
        );
    }

    #[test]
    fn absent_tns_column_does_not_break_wns() {
        let report = "    WNS(ns)\n      2.345\n";
        assert_close(
            FpgaMetrics::parse_from_report(report).expect("WNS not parsed"),
            2.345,
        );
        assert!(FpgaMetrics::parse_tns_from_report(report).is_none());
    }

    #[test]
    fn non_numeric_tns_returns_none_without_losing_wns() {
        let report = "    WNS(ns)      TNS(ns)\n      2.345          N/A\n";
        assert_close(
            FpgaMetrics::parse_from_report(report).expect("WNS not parsed"),
            2.345,
        );
        assert!(FpgaMetrics::parse_tns_from_report(report).is_none());
    }

    #[test]
    fn tns_without_header_returns_none() {
        assert!(FpgaMetrics::parse_tns_from_report("      2.345       -1.882\n").is_none());
        assert!(FpgaMetrics::parse_tns_from_report("").is_none());
    }

    #[test]
    fn column_rule_alone_after_header_returns_none() {
        let report = "    WNS(ns)      TNS(ns)\n    -------      -------\n";
        assert!(FpgaMetrics::parse_from_report(report).is_none());
        assert!(FpgaMetrics::parse_tns_from_report(report).is_none());
    }

    #[test]
    fn parses_lut_utilization_from_utilization_report() {
        let lut = FpgaMetrics::parse_lut_utilization(UTILIZATION_REPORT).expect("LUT row missing");
        assert_close(lut, 0.153);
    }

    #[test]
    fn parses_lut_row_with_asterisk_and_extra_column() {
        // Newer Vivado emits a `Prohibited` column and stars constrained rows.
        let report = "\
| Site Type       | Used | Fixed | Prohibited | Available | Util% |
| Slice LUTs*     | 4160 |     0 |          0 |     20800 | 20.00 |
";
        assert_close(
            FpgaMetrics::parse_lut_utilization(report).expect("LUT row missing"),
            0.20,
        );
    }

    #[test]
    fn parses_clb_lut_row_for_ultrascale() {
        let report = "| CLB LUTs | 1040 | 0 | 20800 | 5.00 |\n";
        assert_close(
            FpgaMetrics::parse_lut_utilization(report).expect("CLB LUT row missing"),
            0.05,
        );
    }

    #[test]
    fn nested_lut_rows_alone_are_ignored() {
        let report = "\
|   LUT as Logic  | 3050 |     0 |     20800 | 14.66 |
|   LUT as Memory |  132 |     0 |      9600 |  1.38 |
";
        assert!(FpgaMetrics::parse_lut_utilization(report).is_none());
    }

    #[test]
    fn timing_report_has_no_lut_utilization() {
        assert!(FpgaMetrics::parse_lut_utilization(TIMING_SUMMARY_WITH_RULE).is_none());
    }

    #[test]
    fn non_numeric_lut_percentage_returns_none() {
        let report = "| Slice LUTs | 3182 | 0 | 20800 | n/a |\n";
        assert!(FpgaMetrics::parse_lut_utilization(report).is_none());
    }

    #[test]
    fn load_from_path_populates_tns_and_defaults_lut_to_zero() {
        let file = report_file(TIMING_SUMMARY_WITH_RULE);
        let metrics = FpgaMetrics::load_from_path(path_of(&file)).expect("metrics not loaded");
        assert_close(metrics.wns_ns, 2.345);
        assert_close(metrics.tns_ns, -1.882);
        assert_close(metrics.lut_utilization, 0.0);
        assert!(metrics.synthesis_ok);
    }

    #[test]
    fn load_from_path_reads_lut_from_a_combined_report() {
        let combined = format!("{TIMING_SUMMARY_WITH_RULE}\n{UTILIZATION_REPORT}");
        let file = report_file(&combined);
        let metrics = FpgaMetrics::load_from_path(path_of(&file)).expect("metrics not loaded");
        assert_close(metrics.wns_ns, 2.345);
        assert_close(metrics.lut_utilization, 0.153);
    }

    #[test]
    fn load_from_reports_populates_all_fields() {
        let timing = report_file(TIMING_SUMMARY_WITH_RULE);
        let utilization = report_file(UTILIZATION_REPORT);
        let metrics = FpgaMetrics::load_from_reports(path_of(&timing), path_of(&utilization))
            .expect("metrics not loaded");
        assert_close(metrics.wns_ns, 2.345);
        assert_close(metrics.tns_ns, -1.882);
        assert_close(metrics.lut_utilization, 0.153);
        assert!(metrics.synthesis_ok);
    }

    #[test]
    fn load_from_reports_tolerates_a_missing_utilization_report() {
        let timing = report_file(TIMING_SUMMARY_WITH_RULE);
        let metrics = FpgaMetrics::load_from_reports(path_of(&timing), "does/not/exist.rpt")
            .expect("timing metrics should survive a missing utilization report");
        assert_close(metrics.wns_ns, 2.345);
        assert_close(metrics.tns_ns, -1.882);
        assert_close(metrics.lut_utilization, 0.0);
    }

    #[test]
    fn load_from_reports_returns_none_without_a_timing_report() {
        let utilization = report_file(UTILIZATION_REPORT);
        assert!(
            FpgaMetrics::load_from_reports("does/not/exist.rpt", path_of(&utilization)).is_none()
        );
    }

    #[test]
    fn default_metrics_are_zeroed() {
        let metrics = FpgaMetrics::default();
        assert_close(metrics.wns_ns, 0.0);
        assert_close(metrics.tns_ns, 0.0);
        assert_close(metrics.lut_utilization, 0.0);
        assert!(!metrics.synthesis_ok);
    }

    #[test]
    fn metrics_round_trip_through_json() {
        let metrics = FpgaMetrics {
            wns_ns: 2.345,
            tns_ns: -1.882,
            lut_utilization: 0.153,
            synthesis_ok: true,
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        let decoded: FpgaMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_close(decoded.wns_ns, 2.345);
        assert_close(decoded.tns_ns, -1.882);
        assert_close(decoded.lut_utilization, 0.153);
        assert!(decoded.synthesis_ok);
    }
}
