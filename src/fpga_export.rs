// SPDX-License-Identifier: MIT OR Apache-2.0
//! FPGA parameter export for [silicon-hdl](https://github.com/Limen-Neural/silicon-hdl).
//!
//! Converts trained SNN floats to unsigned **Q8.8** (`u16`) vectors and writes
//! Vivado `$readmemh` `.mem` files consumable by `WeightRam` and `NeuronParamRam`
//! in the silicon-hdl core library.
//!
//! ## Traits
//!
//! Hardware-facing crates should depend on the traits here rather than the
//! concrete [`FpgaParameterExporter`] type when possible:
//!
//! - [`FixedPointEncode`] — `f32` → Q8.8 `u16`
//! - [`ParameterExport`] — produce [`FpgaParameters`]
//! - [`MemFileWriter`] — write `.mem` + metadata JSON

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Metadata / layout tag for the Q8.8 `.mem` bundle shared with silicon-hdl.
///
/// Historical name from the Spikenaut deployment pipeline; kept for downstream
/// tooling that keys on this string.
pub const EXPORT_FORMAT_VERSION: &str = "Spikenaut-v2";

/// Encode host floating-point values as unsigned Q8.8 (`u16`).
///
/// Q8.8 maps `value × 256` into a 16-bit word. Values outside the representable
/// range are clamped.
pub trait FixedPointEncode {
    /// Convert one `f32` to Q8.8 fixed-point.
    fn encode_q88(&self, value: f32) -> u16;
}

/// Export SNN parameters as an FPGA-facing Q8.8 parameter bundle.
///
/// The resulting [`FpgaParameters`] align with silicon-hdl RAM contents
/// (`WeightRam`, `NeuronParamRam`).
pub trait ParameterExport {
    /// Build the full Q8.8 parameter set and metadata.
    fn export(&self) -> FpgaParameters;
}

/// Write Q8.8 parameter vectors as Vivado `$readmemh` `.mem` files.
pub trait MemFileWriter {
    /// Error type for filesystem / I/O failures.
    type Error;

    /// Write `parameters.mem`, `parameters_weights.mem`, `parameters_decay.mem`,
    /// and `parameters.json` under `output_dir`.
    fn write_mem_files(&self, output_dir: impl AsRef<Path>) -> Result<(), Self::Error>;
}

/// Default FPGA parameter exporter for the silicon-hdl Q8.8 layout.
///
/// Exports learned SNN parameters in Q8.8 fixed-point format for FPGA
/// deployment with a &lt;35µs/tick target latency budget.
pub struct FpgaParameterExporter {
    thresholds: Vec<f32>,
    weights: Vec<Vec<f32>>,
    decay_rates: Vec<f32>,
}

/// FPGA-compatible parameter format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpgaParameters {
    /// Neuron thresholds in Q8.8 format
    pub thresholds: Vec<u16>,
    /// Weight matrix [neurons x channels] in Q8.8 format  
    pub weights: Vec<u16>,
    /// Decay rates in Q8.8 format
    pub decay_rates: Vec<u16>,
    /// Metadata about the parameter set
    pub metadata: FpgaMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpgaMetadata {
    pub version: String,
    pub timestamp: String,
    pub num_neurons: usize,
    pub num_channels: usize,
    pub target_latency_us: f32,
    pub memory_usage_kb: f32,
}

impl FpgaParameterExporter {
    /// Create new exporter with default parameters
    pub fn new() -> Self {
        Self {
            thresholds: Vec::new(),
            weights: Vec::new(),
            decay_rates: Vec::new(),
        }
    }

    /// Set neuron thresholds
    pub fn set_thresholds(&mut self, thresholds: Vec<f32>) {
        self.thresholds = thresholds;
    }

    /// Set weight matrix [neurons x channels]
    pub fn set_weights(&mut self, weights: Vec<Vec<f32>>) {
        self.weights = weights;
    }

    /// Set decay rates
    pub fn set_decay_rates(&mut self, decay_rates: Vec<f32>) {
        self.decay_rates = decay_rates;
    }

    /// Convert `f32` to Q8.8 fixed-point format.
    ///
    /// Prefer [`FixedPointEncode::encode_q88`] when coding against the trait.
    pub fn to_q88(&self, value: f32) -> u16 {
        self.encode_q88(value)
    }

    /// Export parameters to FPGA-compatible format.
    ///
    /// Prefer [`ParameterExport::export`] when coding against the trait.
    pub fn export(&self) -> FpgaParameters {
        ParameterExport::export(self)
    }

    /// Export parameters to `.mem` files for silicon-hdl / Vivado `$readmemh`.
    ///
    /// Prefer [`MemFileWriter::write_mem_files`] when coding against the trait.
    pub fn export_to_mem_files<P: AsRef<Path>>(
        &self,
        output_dir: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.write_mem_files(output_dir)
    }

    /// Create an exporter pre-populated with given parameters.
    pub fn from_params(
        thresholds: Vec<f32>,
        weights: Vec<Vec<f32>>,
        decay_rates: Vec<f32>,
    ) -> Self {
        Self {
            thresholds,
            weights,
            decay_rates,
        }
    }

    fn calculate_memory_usage(&self) -> f32 {
        let total_params = self.thresholds.len()
            + self.weights.iter().map(|row| row.len()).sum::<usize>()
            + self.decay_rates.len();

        // Each parameter is 2 bytes (u16) in Q8.8 format
        (total_params * 2) as f32 / 1024.0
    }

    fn write_mem_file(
        path: impl AsRef<Path>,
        values: &[u16],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = fs::File::create(path)?;
        for value in values {
            writeln!(file, "{:04X}", value)?;
        }
        Ok(())
    }

    fn print_export_summary<P: AsRef<Path>>(&self, params: &FpgaParameters, output_dir: P) {
        println!("=== FPGA Parameter Export Summary ===");
        println!("Output Directory: {}", output_dir.as_ref().display());
        println!("Version: {}", params.metadata.version);
        println!("Timestamp: {}", params.metadata.timestamp);
        println!("Neurons: {}", params.metadata.num_neurons);
        println!("Channels: {}", params.metadata.num_channels);
        println!("Target Latency: {:.1}µs", params.metadata.target_latency_us);
        println!("Memory Usage: {:.2} KB", params.metadata.memory_usage_kb);
        println!();
        println!("Files Generated:");
        println!(
            "  parameters.mem         - {} thresholds",
            params.thresholds.len()
        );
        println!(
            "  parameters_weights.mem - {} weights",
            params.weights.len()
        );
        println!(
            "  parameters_decay.mem   - {} decay rates",
            params.decay_rates.len()
        );
        println!("  parameters.json        - metadata and configuration");
        println!();
        println!("SUCCESS: FPGA parameters ready for silicon-hdl deployment");
    }
}

impl FixedPointEncode for FpgaParameterExporter {
    fn encode_q88(&self, value: f32) -> u16 {
        // Q8.8: 8 integer bits, 8 fractional bits
        // Range: 0.0 to 255.996 (clamped into u16)
        let scaled = value * 256.0;
        scaled.clamp(0.0, 65535.0) as u16
    }
}

impl ParameterExport for FpgaParameterExporter {
    fn export(&self) -> FpgaParameters {
        let thresholds_q88: Vec<u16> = self
            .thresholds
            .iter()
            .map(|&v| self.encode_q88(v))
            .collect();

        let weights_q88: Vec<u16> = self
            .weights
            .iter()
            .flat_map(|row| row.iter())
            .map(|&v| self.encode_q88(v))
            .collect();

        let decay_rates_q88: Vec<u16> = self
            .decay_rates
            .iter()
            .map(|&v| self.encode_q88(v))
            .collect();

        let metadata = FpgaMetadata {
            version: EXPORT_FORMAT_VERSION.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            num_neurons: self.thresholds.len(),
            num_channels: if self.weights.is_empty() {
                0
            } else {
                self.weights[0].len()
            },
            target_latency_us: 35.0,
            memory_usage_kb: self.calculate_memory_usage(),
        };

        FpgaParameters {
            thresholds: thresholds_q88,
            weights: weights_q88,
            decay_rates: decay_rates_q88,
            metadata,
        }
    }
}

impl MemFileWriter for FpgaParameterExporter {
    type Error = Box<dyn std::error::Error>;

    fn write_mem_files(&self, output_dir: impl AsRef<Path>) -> Result<(), Self::Error> {
        fs::create_dir_all(&output_dir)?;

        let params = ParameterExport::export(self);

        Self::write_mem_file(
            output_dir.as_ref().join("parameters.mem"),
            &params.thresholds,
        )?;
        Self::write_mem_file(
            output_dir.as_ref().join("parameters_weights.mem"),
            &params.weights,
        )?;
        Self::write_mem_file(
            output_dir.as_ref().join("parameters_decay.mem"),
            &params.decay_rates,
        )?;

        let metadata_path = output_dir.as_ref().join("parameters.json");
        let metadata_json = serde_json::to_string_pretty(&params)?;
        fs::write(metadata_path, metadata_json)?;

        self.print_export_summary(&params, output_dir);

        Ok(())
    }
}

impl Default for FpgaParameterExporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to format Q8.8 value as hex string
pub fn format_q88_hex(value: f32) -> String {
    let exporter = FpgaParameterExporter::new();
    let q88_value = exporter.encode_q88(value);
    format!("{:04X}", q88_value)
}

/// Helper function to convert Q8.8 back to f32
pub fn q88_to_f32(q88_value: u16) -> f32 {
    q88_value as f32 / 256.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_q88_conversion() {
        let exporter = FpgaParameterExporter::new();

        // Test basic conversions
        assert_eq!(exporter.to_q88(0.0), 0);
        assert_eq!(exporter.to_q88(1.0), 256);
        assert_eq!(exporter.to_q88(255.0), 65280);

        // Test precision
        assert_eq!(q88_to_f32(256), 1.0);
        assert_eq!(q88_to_f32(0), 0.0);
        assert_eq!(q88_to_f32(65280), 255.0);
    }

    #[test]
    fn test_parameter_export() {
        let mut exporter = FpgaParameterExporter::new();

        // Set test parameters
        exporter.set_thresholds(vec![1.0, 0.8, 1.2]);
        exporter.set_weights(vec![
            vec![0.5, 1.0, 0.3],
            vec![0.7, 0.9, 1.1],
            vec![0.4, 0.6, 0.8],
        ]);
        exporter.set_decay_rates(vec![0.85, 0.9, 0.8]);

        // Export to FPGA format
        let params = exporter.export();

        // Verify conversion
        assert_eq!(params.thresholds.len(), 3);
        assert_eq!(params.weights.len(), 9); // 3x3
        assert_eq!(params.decay_rates.len(), 3);
        assert_eq!(params.metadata.num_neurons, 3);
        assert_eq!(params.metadata.num_channels, 3);

        // Verify converted Q8.8 values are correct
        assert_eq!(params.thresholds, vec![256, 204, 307]);
        assert_eq!(
            params.weights,
            vec![128, 256, 76, 179, 230, 281, 102, 153, 204]
        );
        assert_eq!(params.decay_rates, vec![217, 230, 204]);
    }

    #[test]
    fn test_memory_calculation() {
        let mut exporter = FpgaParameterExporter::new();

        // Set parameters for 16 neurons, 16 channels
        exporter.set_thresholds(vec![1.0; 16]);
        exporter.set_weights(vec![vec![0.5; 16]; 16]);
        exporter.set_decay_rates(vec![0.85; 16]);

        let params = exporter.export();

        // Expected memory: (16 + 256 + 16) * 2 bytes = 576 bytes = 0.5625 KB
        assert!((params.metadata.memory_usage_kb - 0.5625).abs() < 0.01);
    }

    #[test]
    fn test_trait_surface() {
        let exporter = FpgaParameterExporter::from_params(vec![1.0], vec![vec![0.5]], vec![0.9]);

        // Trait methods (not only inherent methods) are the stable hardware surface.
        assert_eq!(FixedPointEncode::encode_q88(&exporter, 1.0), 256);
        let params = ParameterExport::export(&exporter);
        assert_eq!(params.metadata.version, EXPORT_FORMAT_VERSION);
        assert_eq!(params.thresholds, vec![256]);
        assert_eq!(params.weights, vec![128]);
        assert_eq!(params.decay_rates, vec![230]);
    }

    /// Small fixture whose floats are exact Q8.8 multiples of 1/256.
    fn mem_writer_fixture() -> FpgaParameterExporter {
        FpgaParameterExporter::from_params(
            vec![1.0, 0.75],
            vec![vec![0.5, 2.0], vec![0.25, 1.5]],
            vec![0.5, 0.75],
        )
    }

    fn read_mem_lines(path: impl AsRef<Path>) -> Vec<String> {
        fs::read_to_string(path)
            .expect("mem file should be readable")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn assert_uppercase_hex_words(lines: &[String]) {
        for line in lines {
            assert_eq!(line.len(), 4, "expected XXXX hex word, got {line:?}");
            assert!(
                line.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()),
                "expected uppercase XXXX hex word, got {line:?}"
            );
        }
    }

    fn parse_mem_words(lines: &[String]) -> Vec<u16> {
        lines
            .iter()
            .map(|line| u16::from_str_radix(line, 16).expect("mem line should be valid hex"))
            .collect()
    }

    #[test]
    fn test_write_mem_files_via_trait_emits_expected_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exporter = mem_writer_fixture();

        MemFileWriter::write_mem_files(&exporter, dir.path()).expect("write_mem_files");

        let mut names: Vec<String> = fs::read_dir(dir.path())
            .expect("output dir")
            .map(|entry| {
                entry
                    .expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "parameters.json",
                "parameters.mem",
                "parameters_decay.mem",
                "parameters_weights.mem",
            ]
        );

        let thresholds = read_mem_lines(dir.path().join("parameters.mem"));
        let weights = read_mem_lines(dir.path().join("parameters_weights.mem"));
        let decay_rates = read_mem_lines(dir.path().join("parameters_decay.mem"));

        assert_uppercase_hex_words(&thresholds);
        assert_uppercase_hex_words(&weights);
        assert_uppercase_hex_words(&decay_rates);

        // 1.0 -> 0x0100, 0.75 -> 0x00C0 (letter digit proves uppercase)
        assert_eq!(thresholds, ["0100", "00C0"]);
        // Flattened row-major: 0.5, 2.0, 0.25, 1.5
        assert_eq!(weights, ["0080", "0200", "0040", "0180"]);
        assert_eq!(decay_rates, ["0080", "00C0"]);
    }

    #[test]
    fn test_mem_files_round_trip_to_parameter_export() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exporter = mem_writer_fixture();

        MemFileWriter::write_mem_files(&exporter, dir.path()).expect("write_mem_files");

        let expected = ParameterExport::export(&exporter);
        let thresholds = parse_mem_words(&read_mem_lines(dir.path().join("parameters.mem")));
        let weights = parse_mem_words(&read_mem_lines(
            dir.path().join("parameters_weights.mem"),
        ));
        let decay_rates = parse_mem_words(&read_mem_lines(dir.path().join("parameters_decay.mem")));

        assert_eq!(thresholds, expected.thresholds);
        assert_eq!(weights, expected.weights);
        assert_eq!(decay_rates, expected.decay_rates);

        let decoded: Vec<f32> = thresholds.iter().copied().map(q88_to_f32).collect();
        for (got, want) in decoded.iter().zip([1.0_f32, 0.75]) {
            assert!(
                (got - want).abs() < 1e-6,
                "Q8.8 round-trip drifted: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn test_metadata_json_round_trips_to_fpga_parameters() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exporter = mem_writer_fixture();

        MemFileWriter::write_mem_files(&exporter, dir.path()).expect("write_mem_files");

        let json = fs::read_to_string(dir.path().join("parameters.json")).expect("parameters.json");
        let round_tripped: FpgaParameters =
            serde_json::from_str(&json).expect("parameters.json should deserialize");

        let expected = ParameterExport::export(&exporter);
        assert_eq!(round_tripped.thresholds, expected.thresholds);
        assert_eq!(round_tripped.weights, expected.weights);
        assert_eq!(round_tripped.decay_rates, expected.decay_rates);

        assert_eq!(round_tripped.metadata.version, EXPORT_FORMAT_VERSION);
        assert_eq!(round_tripped.metadata.num_neurons, 2);
        assert_eq!(round_tripped.metadata.num_channels, 2);
        assert!(!round_tripped.metadata.timestamp.is_empty());
        assert!((round_tripped.metadata.target_latency_us - 35.0).abs() < 1e-6);
        // (2 thresholds + 4 weights + 2 decay) * 2 bytes = 16 bytes
        assert!((round_tripped.metadata.memory_usage_kb - 16.0 / 1024.0).abs() < 1e-6);
    }
}
