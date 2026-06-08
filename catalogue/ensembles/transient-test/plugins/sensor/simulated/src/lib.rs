//! # r2-plugin-sensor-simulated
//!
//! R2 AOT plugin: a **deterministic synthetic triaxial-acceleration
//! source** for the transient-networking hardware test tier (Phase 3
//! Part D4a). It has **no hardware dependency** — it produces a
//! repeatable integer waveform so the test ensemble has a data source on
//! every DFR1195 (and the laptop) before real sensors are wired.
//!
//! It is a **drop-in** for `ai.reality2.cap.accel.triaxial`: `read`
//! returns the same 12-byte `[x|y|z]` (i32 LE, 256_000-LSB/g convention)
//! as `sensor/adxl355` and `sensor/lis2dh`, so the `Accelerometer`
//! sentant consumes it unchanged — the swap lever (R2-PLUGIN §10) lets a
//! device run on simulated data, then swap to a real chip with no sentant
//! change.
//!
//! Determinism matters for the test tier: injecting "the sample at step
//! N" on one node and asserting it arrives on another requires the source
//! be reproducible. No RNG, no float, no clock — output is a pure
//! function of an internal step counter.
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name  | Input | Output |
//! |------|-------|-------|--------|
//! | 0x01 | init  | `[]`  | `[]`   |
//! | 0x02 | read  | `[]`  | `[x i32 LE | y i32 LE | z i32 LE]` (12 bytes) + advances the step |
//! | 0x03 | reset | `[]`  | `[]` (step → 0; reproduce from the top) |

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Command opcode: initialise (reset the step counter).
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: read one synthetic sample (advances the step).
pub const CMD_READ: PluginCommand = 0x02;
/// Command opcode: reset the step counter to 0.
pub const CMD_RESET: PluginCommand = 0x03;

/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// 1 g in the shared LSB-per-g convention (matches adxl355 / lis2dh).
pub const ONE_G_LSB: i32 = 256_000;
/// Period (in steps) of the X-axis triangle sweep.
const X_PERIOD: u32 = 100;
/// Period of the small Z wobble around 1 g.
const Z_PERIOD: u32 = 40;
/// Amplitude of the Z wobble.
const Z_WOBBLE: i32 = 8_000;

/// Deterministic synthetic triaxial source.
pub struct SimulatedSensor {
    id: PluginId,
    step: u32,
}

impl SimulatedSensor {
    /// Construct bound to `id`, at step 0.
    pub const fn new(id: PluginId) -> Self {
        Self { id, step: 0 }
    }

    /// The synthetic sample for a given step — a pure function, so the
    /// same step always yields the same `(x, y, z)`. X sweeps a triangle
    /// across ±1 g; Y is held at 0; Z sits at 1 g (gravity) with a small
    /// triangle wobble. Exposed for assertions / cross-node expectations.
    pub fn sample_at(step: u32) -> (i32, i32, i32) {
        let x = triangle(step, X_PERIOD, ONE_G_LSB);
        let y = 0;
        let z = ONE_G_LSB + triangle(step, Z_PERIOD, Z_WOBBLE);
        (x, y, z)
    }

    fn op_read(&mut self) -> PluginResult {
        let (x, y, z) = Self::sample_at(self.step);
        self.step = self.step.wrapping_add(1);
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&x.to_le_bytes());
        out[4..8].copy_from_slice(&y.to_le_bytes());
        out[8..12].copy_from_slice(&z.to_le_bytes());
        PluginResult::Ok(PluginResponse::with_data(&out))
    }
}

impl Plugin for SimulatedSensor {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT | CMD_RESET => {
                self.step = 0;
                PluginResult::Ok(PluginResponse::empty())
            }
            CMD_READ => self.op_read(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "simulated: unknown command byte")),
        }
    }
    fn name(&self) -> &str {
        "sensor/simulated"
    }
    fn id(&self) -> PluginId {
        self.id
    }
    fn init(&mut self) -> PluginResult {
        self.step = 0;
        PluginResult::Ok(PluginResponse::empty())
    }
}

/// Symmetric triangle wave: ramps `-amp → +amp → -amp` over `period`
/// steps. Integer-only (no float), so it is exact + `no_std`-clean.
fn triangle(step: u32, period: u32, amp: i32) -> i32 {
    let half = (period / 2).max(1);
    let p = step % period;
    // v ramps 0..half..0 across the period.
    let v = if p <= half { p } else { period - p };
    // map v∈[0,half] → [-amp, +amp]
    let span = (2 * amp as i64) * (v as i64) / (half as i64);
    (-(amp as i64) + span) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(p: &mut SimulatedSensor) -> (i32, i32, i32) {
        let PluginResult::Ok(resp) = p.execute(CMD_READ, &[]) else { panic!("read") };
        let b = resp.as_slice();
        (
            i32::from_le_bytes(b[0..4].try_into().unwrap()),
            i32::from_le_bytes(b[4..8].try_into().unwrap()),
            i32::from_le_bytes(b[8..12].try_into().unwrap()),
        )
    }

    #[test]
    fn read_is_12_bytes_and_advances() {
        let mut p = SimulatedSensor::new(7);
        p.execute(CMD_INIT, &[]);
        let s0 = read(&mut p);
        let s1 = read(&mut p);
        // distinct consecutive samples (the waveform moves)
        assert_ne!(s0, s1);
    }

    #[test]
    fn deterministic_same_step_same_sample() {
        // Two independent instances produce identical streams — the
        // property the test tier relies on for inject-here/expect-there.
        let mut a = SimulatedSensor::new(1);
        let mut b = SimulatedSensor::new(2);
        a.execute(CMD_INIT, &[]);
        b.execute(CMD_INIT, &[]);
        for _ in 0..250 {
            assert_eq!(read(&mut a), read(&mut b));
        }
    }

    #[test]
    fn reset_reproduces_from_top() {
        let mut p = SimulatedSensor::new(7);
        p.execute(CMD_INIT, &[]);
        let first = read(&mut p);
        for _ in 0..50 { read(&mut p); }
        p.execute(CMD_RESET, &[]);
        assert_eq!(read(&mut p), first);
    }

    #[test]
    fn sample_at_is_pure() {
        assert_eq!(SimulatedSensor::sample_at(42), SimulatedSensor::sample_at(42));
    }

    #[test]
    fn waveform_stays_in_physical_range() {
        // X within ±1 g; Z within 1 g ± wobble; Y flat. Sanity-bounds the
        // synthetic motion so it looks like a real chip's output.
        for step in 0..500 {
            let (x, y, z) = SimulatedSensor::sample_at(step);
            assert!(x >= -ONE_G_LSB && x <= ONE_G_LSB, "x out of range at {step}: {x}");
            assert_eq!(y, 0);
            assert!(z >= ONE_G_LSB - Z_WOBBLE && z <= ONE_G_LSB + Z_WOBBLE, "z at {step}: {z}");
        }
    }

    #[test]
    fn triangle_endpoints() {
        // At phase 0 → -amp; at half period → +amp.
        assert_eq!(triangle(0, 100, ONE_G_LSB), -ONE_G_LSB);
        assert_eq!(triangle(50, 100, ONE_G_LSB), ONE_G_LSB);
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = SimulatedSensor::new(7);
        let PluginResult::Error(e) = p.execute(0xAA, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}
