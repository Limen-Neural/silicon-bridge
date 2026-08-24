// SPDX-License-Identifier: MIT OR Apache-2.0
//! FPGA Spike Readback — UART Bridge to Basys3 Hardware
//!
//! Handles UART communication with Basys3 FPGA to send stimuli
//! and read back spike states using the SiliconBridge v3.0 protocol.
//!
//! Extracted from Eagle-Lander's Ship of Theseus neuromorphic core.
//!
//! ## Q8.8 convention used here: signed
//!
//! Host stimuli and membrane potentials on the wire are **signed** Q8.8 (`i16`,
//! two's complement, big-endian). This is *not* the convention used by the
//! parameter export path (`fpga_export`), which is unsigned `u16` written as
//! ASCII hex. Encoding a stimulus with the export encoder would turn every
//! negative (inhibitory) input into `0`; decoding a potential with the export
//! decoder would read negatives as large positives.
//!
//! | Aspect | This module (UART TX/RX) | `fpga_export` (`.mem`) |
//! |---|---|---|
//! | Encode with | [`encode_q88_signed`] | `encode_q88_unsigned` |
//! | Decode with | [`q88_signed_to_f32`] | `q88_to_f32` |
//! | Raw type | `i16` (two's complement) | `u16` (unsigned) |
//! | Width | 16 bits — 8 integer + 8 fractional | 16 bits — 8 integer + 8 fractional |
//! | Encoder input clamp | [`STIMULUS_Q88_MIN`]`..=`[`STIMULUS_Q88_MAX`] (`-127.99..=127.99`) | `0.0..=255.99609375` |
//! | Encoder raw output | `-32765..=32765` (TX saturation) | `0..=65535` |
//! | Decoder accepts | any `i16`: `-32768..=32767` → `-128.0..=127.99609375` | any `u16`: `0..=65535` |
//! | Byte order | raw binary, big-endian (MSB first) | ASCII hex, one `{:04X}` word per line |
//! | Use it for | host stimuli, RX membrane potentials | weights, thresholds, decay rates |
//!
//! The crate root docs carry the same table as the canonical reference.

use serialport::{SerialPort, SerialPortInfo};
use std::io::{Read, Write};
use std::time::Duration;

/// Lower clamp bound for host stimuli on the signed Q8.8 UART path.
///
/// Values below this saturate to raw `-32765` on the wire.
pub const STIMULUS_Q88_MIN: f32 = -127.99;

/// Upper clamp bound for host stimuli on the signed Q8.8 UART path.
///
/// Values above this saturate to raw `32765` on the wire.
pub const STIMULUS_Q88_MAX: f32 = 127.99;

/// Encode a host stimulus as **signed** Q8.8 (`i16`, two's complement).
///
/// `raw = value × 256`, truncated toward zero, with the input clamped to
/// [`STIMULUS_Q88_MIN`]`..=`[`STIMULUS_Q88_MAX`]. `NaN` encodes as `0`. The
/// wire format is big-endian, so callers serialize with `to_be_bytes()`.
///
/// This is the stimulus counterpart of the crate's unsigned `encode_q88_unsigned`
/// used for `.mem` parameter export — the two are **not** interchangeable.
///
/// ```rust
/// use silicon_bridge::{encode_q88_signed, q88_signed_to_f32};
///
/// assert_eq!(encode_q88_signed(1.0), 256);
/// assert_eq!(encode_q88_signed(-1.0), -256); // negatives survive here
/// assert_eq!(encode_q88_signed(-1.0).to_be_bytes(), [0xFF, 0x00]);
/// assert_eq!(q88_signed_to_f32(encode_q88_signed(-0.5)), -0.5);
/// ```
pub fn encode_q88_signed(value: f32) -> i16 {
    // ENCODE SITE (signed Q8.8) — this is the UART / host-stimulus path.
    // Clamp happens on the *unscaled* value, so saturation lands on raw
    // ±32765 (±127.99 × 256, truncated) rather than the i16 limits.
    (value.clamp(STIMULUS_Q88_MIN, STIMULUS_Q88_MAX) * 256.0) as i16
}

/// Decode a **signed** Q8.8 (`i16`) wire word back to `f32`.
///
/// Counterpart of [`encode_q88_signed`], but deliberately wider: every `i16` the
/// FPGA can send is valid, so this covers `-32768..=32767`
/// (`-128.0..=127.99609375`) — including the two words just outside what
/// [`encode_q88_signed`] can produce (it saturates at `±32765`).
///
/// For `.mem` parameter words use the crate's unsigned `q88_to_f32` instead.
pub fn q88_signed_to_f32(raw: i16) -> f32 {
    raw as f32 / 256.0
}

pub struct FpgaBridge {
    port: Box<dyn SerialPort>,
    active: bool,
}

impl FpgaBridge {
    /// Try to open FPGA connection on available USB ports
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Try common USB ports for Basys3
        let ports = ["/dev/ttyUSB0", "/dev/ttyUSB1", "/dev/ttyUSB2"];

        for port_name in &ports {
            match serialport::new(*port_name, 115_200)
                .timeout(Duration::from_millis(100))
                .open()
            {
                Ok(port) => {
                    println!("[fpga] Connected to FPGA on {}", port_name);
                    return Ok(FpgaBridge { port, active: true });
                }
                Err(_) => continue,
            }
        }

        Err("FPGA not found on any USB port".into())
    }

    /// Send neural stimuli to FPGA and read back spike states.
    ///
    /// Protocol (16-neuron SiliconBridge v3.0):
    ///   TX: 0xAA + 32 bytes (16 × signed Q8.8 stimuli, big-endian)
    ///   RX: 32 bytes (16 × signed Q8.8 potentials) + 2 bytes (spike flags) + 2 bytes (switches)
    ///
    /// Stimuli are encoded with [`encode_q88_signed`] (`i16`, clamped to
    /// [`STIMULUS_Q88_MIN`]`..=`[`STIMULUS_Q88_MAX`]) — **not** with the
    /// unsigned `.mem` export encoder. RX potentials are decoded with
    /// [`q88_signed_to_f32`] over the full `i16` range, which is wider than what
    /// the TX encoder can emit.
    ///
    /// Input is accepted as a dynamic slice; if fewer than 16 values are provided,
    /// remaining channels are zero-padded. If more are provided, only the first 16 are sent.
    pub fn process_stimuli(
        &mut self,
        stimuli: &[f32],
    ) -> Result<(Vec<f32>, Vec<bool>), Box<dyn std::error::Error>> {
        if !self.active {
            return Err("FPGA bridge not active".into());
        }

        // Convert stimuli to SIGNED Q8.8 (i16), big-endian on the wire.
        // Note this is the signed convention, not the unsigned `u16` one used
        // by `fpga_export` for `.mem` files — see this module's docs.
        let mut tx_data = vec![0xAAu8]; // Sync byte
        for i in 0..16 {
            let s = stimuli.get(i).copied().unwrap_or(0.0);
            let q8_8 = encode_q88_signed(s);
            tx_data.extend_from_slice(&q8_8.to_be_bytes());
        }

        // Send to FPGA
        self.port.write_all(&tx_data)?;
        self.port.flush()?;

        // Read response: 32 bytes potentials + 2 bytes spike flags + 2 bytes switches
        let mut rx_data = vec![0u8; 36];
        self.port.read_exact(&mut rx_data)?;

        // Parse potentials (signed Q8.8 big-endian back to f32) — membrane
        // potentials can be negative, hence `i16` rather than the export `u16`.
        let mut potentials = Vec::with_capacity(16);
        for i in 0..16 {
            let raw = i16::from_be_bytes([rx_data[i * 2], rx_data[i * 2 + 1]]);
            potentials.push(q88_signed_to_f32(raw));
        }

        // Parse spike flags (16-bit, 1 per neuron)
        let spike_word = u16::from_be_bytes([rx_data[32], rx_data[33]]);
        let spikes = (0..16).map(|i| (spike_word & (1 << i)) != 0).collect();
        // rx_data[34..36] = switch state (available but unused here)

        Ok((potentials, spikes))
    }

    /// Check if FPGA is responsive
    pub fn ping(&mut self) -> bool {
        let test_stimuli = [0.1; 16];
        match self.process_stimuli(&test_stimuli) {
            Ok(_) => true,
            Err(_) => {
                self.active = false;
                false
            }
        }
    }

    /// Get connection status
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Find FPGA ports on the system
pub fn find_fpga_ports() -> Vec<SerialPortInfo> {
    match serialport::available_ports() {
        Ok(ports) => ports
            .into_iter()
            .filter(|p| p.port_name.contains("ttyUSB"))
            .collect(),
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Q8.8 convention tests (signed UART path). This whole module is compiled only
// with the `uart` feature, so these tests are feature-gated by construction.
// The unsigned export counterpart is tested in `fpga_export`.
// ---------------------------------------------------------------------------
#[cfg(all(test, feature = "uart"))]
mod q88_signed_convention_tests {
    use super::*;
    use crate::{encode_q88_unsigned, q88_to_f32};

    #[test]
    fn signed_encode_scales_by_256_in_both_directions() {
        assert_eq!(encode_q88_signed(0.0), 0);
        assert_eq!(encode_q88_signed(1.0), 256);
        assert_eq!(encode_q88_signed(-1.0), -256);
        assert_eq!(encode_q88_signed(0.5), 128);
        assert_eq!(encode_q88_signed(-0.5), -128);
        assert_eq!(encode_q88_signed(1.0 / 256.0), 1); // one LSB
        assert_eq!(encode_q88_signed(-1.0 / 256.0), -1);
    }

    #[test]
    fn signed_encode_truncates_toward_zero() {
        assert_eq!(encode_q88_signed(0.999), 255);
        assert_eq!(encode_q88_signed(-0.999), -255);
    }

    #[test]
    fn signed_encode_clamps_at_both_ends() {
        // Saturation lands on +/-32765 (+/-127.99 x 256 truncated), inside the
        // i16 limits, so the range is symmetric.
        assert_eq!(encode_q88_signed(STIMULUS_Q88_MAX), 32765);
        assert_eq!(encode_q88_signed(STIMULUS_Q88_MIN), -32765);
        assert_eq!(encode_q88_signed(128.0), 32765);
        assert_eq!(encode_q88_signed(-128.0), -32765);
        assert_eq!(encode_q88_signed(1.0e6), 32765);
        assert_eq!(encode_q88_signed(-1.0e6), -32765);
        assert_eq!(encode_q88_signed(f32::MAX), 32765);
        assert_eq!(encode_q88_signed(f32::MIN), -32765);
        assert_eq!(encode_q88_signed(f32::INFINITY), 32765);
        assert_eq!(encode_q88_signed(f32::NEG_INFINITY), -32765);
    }

    #[test]
    fn signed_encode_maps_nan_to_zero() {
        assert_eq!(encode_q88_signed(f32::NAN), 0);
    }

    #[test]
    fn signed_wire_words_are_big_endian() {
        // TX serializes with `to_be_bytes()`: most significant byte first.
        assert_eq!(encode_q88_signed(1.0).to_be_bytes(), [0x01, 0x00]);
        assert_eq!(encode_q88_signed(-1.0).to_be_bytes(), [0xFF, 0x00]);
        assert_eq!(encode_q88_signed(-0.5).to_be_bytes(), [0xFF, 0x80]);
        assert_eq!(
            encode_q88_signed(STIMULUS_Q88_MAX).to_be_bytes(),
            [0x7F, 0xFD]
        );
    }

    #[test]
    fn signed_encode_round_trips_through_q88_signed_to_f32() {
        for value in [-127.0_f32, -12.25, -1.0, -0.00390625, 0.0, 0.5, 64.75] {
            let raw = encode_q88_signed(value);
            assert_eq!(
                q88_signed_to_f32(raw),
                value,
                "round trip failed for {value}"
            );
        }
    }

    #[test]
    fn signed_decoder_covers_the_full_i16_wire_range() {
        // The encoder saturates at +/-32765, but the FPGA may send any i16, so
        // the decoder must accept the two words just outside that range.
        assert_eq!(q88_signed_to_f32(i16::MIN), -128.0);
        assert_eq!(q88_signed_to_f32(i16::MAX), 32767.0 / 256.0);
        assert_eq!(q88_signed_to_f32(-32766), -32766.0 / 256.0);
        assert!(encode_q88_signed(-128.0) > i16::MIN);
        assert!(encode_q88_signed(f32::MAX) < i16::MAX);
    }

    #[test]
    fn signed_and_unsigned_agree_only_on_the_shared_range() {
        // Non-negative inputs up to the signed clamp encode to the same raw bits.
        for value in [0.0_f32, 0.5, 1.0, 64.25, 127.0] {
            assert_eq!(
                i32::from(encode_q88_signed(value)),
                i32::from(encode_q88_unsigned(value)),
                "conventions should agree for {value}"
            );
        }

        // Above the signed clamp the two diverge: the export path keeps scaling.
        assert_eq!(encode_q88_signed(200.0), 32765);
        assert_eq!(encode_q88_unsigned(200.0), 51200);
    }

    #[test]
    fn using_the_wrong_convention_corrupts_negative_values() {
        // This is the whole point of keeping the two encoders distinct: an
        // inhibitory stimulus survives the signed path and is lost on the
        // unsigned one, and a signed wire word decoded as unsigned reads as a
        // large positive value.
        assert_eq!(encode_q88_signed(-1.0), -256);
        assert_eq!(encode_q88_unsigned(-1.0), 0);

        let wire = encode_q88_signed(-1.0);
        assert_eq!(q88_signed_to_f32(wire), -1.0);
        assert_eq!(q88_to_f32(wire as u16), 255.0);
    }
}
