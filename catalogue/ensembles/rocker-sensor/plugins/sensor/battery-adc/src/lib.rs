//! # r2-plugin-sensor-battery-adc
//!
//! R2 AOT plugin: single-cell LiPo battery telemetry from an ADC pin fed
//! by a 100 kΩ/100 kΩ (÷2) resistive divider. Provides the
//! `ai.reality2.workshop.cap.battery` capability — millivolts at the
//! cell terminal plus a state-of-charge percentage.
//!
//! **Reference implementation:** the host-firmware module
//! `r2-workshop/firmware/esp32-s3/devkitc/src/battery.rs`. This crate is
//! the R2-PLUGIN §12 conformant refactor of that module's *portable*
//! core — the median-of-16 filter, the plausibility + stability gates,
//! the divider inverse, and the piecewise state-of-charge curve. The
//! firmware-specific `BatterySim` fallback and esp-idf ADC setup are
//! **not** part of the plugin: simulation is a firmware-render concern,
//! and the ADC itself is abstracted behind the [`AdcChannel`] trait.
//!
//! ## ADC abstraction
//!
//! embedded-hal 1.0 has no stable ADC trait, so — as with
//! `r2-plugin-storage-nvs` (`KvStore`) and `r2-plugin-time-clock`
//! (`Clock`) — this crate defines its own minimal [`AdcChannel`]. The
//! firmware-render step wraps esp-idf-svc's `AdcChannelDriver` (whose
//! `read()` already returns *calibrated millivolts at the pin*) to
//! satisfy it.
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name      | Input | Output |
//! |------|-----------|-------|--------|
//! | 0x01 | init      | `[]`  | `[]`   |
//! | 0x02 | read_mv   | `[]`  | `[cell_mv: u16 LE | percent: u8]` (3 bytes) |
//! | 0x03 | calibrate | `[…]` | declared; not yet implemented (curve calibration lives in the ADC driver layer the trait abstracts) |
//!
//! ## Modes
//!
//! - `aot` — static link into MCU firmware (`no_std`).
//! - `nif` — reserved for a future Linux-SBC NIF build (`false` in
//!   `plugin.toml` today).

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// A single ADC channel that returns **calibrated millivolts at the pin**.
///
/// The plugin is generic over this so it never depends on a specific HAL.
/// The firmware build supplies an impl wrapping the platform ADC driver
/// (esp-idf's `AdcChannelDriver::read()` already returns calibrated mV);
/// tests supply a scripted impl.
pub trait AdcChannel {
    /// Read one calibrated sample, in millivolts at the ADC pin.
    /// `Err(())` signals a transient ADC read failure.
    fn read_mv(&mut self) -> Result<u16, ()>;
}

/// Command opcode: initialise (mark the channel ready).
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: read cell voltage + state-of-charge.
pub const CMD_READ_MV: PluginCommand = 0x02;
/// Command opcode: two-point calibration (declared; see module docs).
pub const CMD_CALIBRATE: PluginCommand = 0x03;

/// Error code: input byte length did not match the command's layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error code: an ADC sample read failed.
pub const ERR_ADC: u8 = 0x02;
/// Error code: cell voltage outside the plausible single-cell band.
pub const ERR_IMPLAUSIBLE: u8 = 0x03;
/// Error code: sample spread too wide (unstable / unbypassed divider).
pub const ERR_UNSTABLE: u8 = 0x04;
/// Error code: `read_mv` issued before `init`.
pub const ERR_NOT_INIT: u8 = 0x05;
/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Raw ADC samples median-filtered per reading (SPEC-R2-WORKSHOP-SENSOR §8.1).
const SAMPLES_PER_READING: usize = 16;

/// Divider inverse: VBATT = pin_mv × 2 (two equal 100 kΩ legs).
const DIVIDER_INVERSE: u16 = 2;

/// Plausible single-cell LiPo band at the cell terminal (post-scaling).
/// Outside this → a floating pin or unfitted divider, not a real cell.
const PLAUSIBLE_MV_MIN: u16 = 2500;
/// Upper plausible bound (see [`PLAUSIBLE_MV_MIN`]).
const PLAUSIBLE_MV_MAX: u16 = 4500;

/// Max sample-to-sample spread (max − min, in pin mV) within one reading.
/// Wider than this means the ADC S/H never settled (high source impedance
/// / missing bypass cap) and the median would be confidently wrong.
const PLAUSIBLE_SPREAD_MV: u16 = 100;

/// Battery-ADC plugin, generic over an [`AdcChannel`].
pub struct BatteryAdc<A: AdcChannel> {
    adc: A,
    initialised: bool,
    id: PluginId,
}

impl<A: AdcChannel> BatteryAdc<A> {
    /// Construct an un-initialised plugin bound to `id`.
    pub const fn new(adc: A, id: PluginId) -> Self {
        Self { adc, initialised: false, id }
    }

    fn op_read_mv(&mut self) -> PluginResult {
        if !self.initialised {
            return PluginResult::Error(PluginError::new(ERR_NOT_INIT, "battery-adc: read before init"));
        }
        let mut samples = [0u16; SAMPLES_PER_READING];
        for slot in samples.iter_mut() {
            match self.adc.read_mv() {
                Ok(mv) => *slot = mv,
                Err(()) => return PluginResult::Error(PluginError::new(ERR_ADC, "battery-adc: sample read failed")),
            }
        }
        samples.sort_unstable();
        let median_pin = samples[SAMPLES_PER_READING / 2];
        let spread = samples[SAMPLES_PER_READING - 1].saturating_sub(samples[0]);
        let cell_mv = median_pin.saturating_mul(DIVIDER_INVERSE);

        if cell_mv < PLAUSIBLE_MV_MIN || cell_mv > PLAUSIBLE_MV_MAX {
            return PluginResult::Error(PluginError::new(
                ERR_IMPLAUSIBLE,
                "battery-adc: cell voltage outside single-cell band (no divider fitted?)",
            ));
        }
        if spread > PLAUSIBLE_SPREAD_MV {
            return PluginResult::Error(PluginError::new(
                ERR_UNSTABLE,
                "battery-adc: sample spread too wide (unstable/unbypassed divider)",
            ));
        }

        let percent = percent_for_mv(cell_mv);
        let mut out = [0u8; 3];
        out[0..2].copy_from_slice(&cell_mv.to_le_bytes());
        out[2] = percent;
        PluginResult::Ok(PluginResponse::with_data(&out))
    }
}

impl<A: AdcChannel> Plugin for BatteryAdc<A> {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => {
                self.initialised = true;
                PluginResult::Ok(PluginResponse::empty())
            }
            CMD_READ_MV => self.op_read_mv(),
            // TODO Phase 1.4-source-extended: two-point calibration. The
            // reference applies curve calibration in the esp-idf ADC
            // driver (below the AdcChannel trait), so there's nothing for
            // the plugin to do at v0.1 — declared in plugin.toml ahead of
            // a future per-unit-offset feature.
            CMD_CALIBRATE => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND,
                "battery-adc: calibrate declared in plugin.toml but not yet implemented",
            )),
            _ => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND,
                "battery-adc: unknown command byte",
            )),
        }
    }

    fn name(&self) -> &str {
        "sensor/battery-adc"
    }

    fn id(&self) -> PluginId {
        self.id
    }

    fn init(&mut self) -> PluginResult {
        self.initialised = true;
        PluginResult::Ok(PluginResponse::empty())
    }
}

/// Piecewise-linear state-of-charge curve per SPEC-R2-WORKSHOP-SENSOR §8.3.
/// Input is cell voltage in mV; output is 0..=100 %.
pub fn percent_for_mv(mv: u16) -> u8 {
    // Anchor points (mV, percent), monotonically increasing in mV.
    const PTS: &[(u16, u8)] = &[
        (3300, 0), (3400, 5), (3500, 10), (3600, 20),
        (3700, 35), (3800, 50), (3900, 65), (4000, 80),
        (4100, 90), (4200, 100),
    ];
    if mv <= PTS[0].0 {
        return 0;
    }
    if mv >= PTS[PTS.len() - 1].0 {
        return 100;
    }
    for w in PTS.windows(2) {
        let (lo_mv, lo_pct) = w[0];
        let (hi_mv, hi_pct) = w[1];
        if mv >= lo_mv && mv <= hi_mv {
            let span_mv = (hi_mv - lo_mv) as u32;
            let span_pct = (hi_pct - lo_pct) as u32;
            let into = (mv - lo_mv) as u32;
            return (lo_pct as u32 + into * span_pct / span_mv) as u8;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted ADC channel: yields a fixed list of pin-mV samples, then
    /// repeats the last one (so a 16-sample read of a steady source can
    /// be expressed as a single value).
    struct ScriptedAdc {
        samples: Vec<u16>,
        idx: usize,
        fail_at: Option<usize>,
    }

    impl ScriptedAdc {
        fn steady(pin_mv: u16) -> Self {
            Self { samples: vec![pin_mv], idx: 0, fail_at: None }
        }
        fn from(samples: Vec<u16>) -> Self {
            Self { samples, idx: 0, fail_at: None }
        }
        fn failing_at(n: usize) -> Self {
            Self { samples: vec![2000], idx: 0, fail_at: Some(n) }
        }
    }

    impl AdcChannel for ScriptedAdc {
        fn read_mv(&mut self) -> Result<u16, ()> {
            if let Some(n) = self.fail_at {
                if self.idx == n {
                    return Err(());
                }
            }
            let v = *self.samples.get(self.idx).unwrap_or(self.samples.last().unwrap());
            self.idx += 1;
            Ok(v)
        }
    }

    fn init_plugin(adc: ScriptedAdc) -> BatteryAdc<ScriptedAdc> {
        let mut p = BatteryAdc::new(adc, 7);
        assert!(matches!(p.execute(CMD_INIT, &[]), PluginResult::Ok(_)));
        p
    }

    #[test]
    fn curve_endpoints() {
        assert_eq!(percent_for_mv(3000), 0);
        assert_eq!(percent_for_mv(3300), 0);
        assert_eq!(percent_for_mv(4200), 100);
        assert_eq!(percent_for_mv(5000), 100);
    }

    #[test]
    fn curve_anchors_and_interpolation() {
        assert_eq!(percent_for_mv(3700), 35);
        assert_eq!(percent_for_mv(3800), 50);
        assert_eq!(percent_for_mv(4100), 90);
        // Halfway 3700(35)→3800(50) ≈ 42 (integer floor).
        assert_eq!(percent_for_mv(3750), 42);
    }

    #[test]
    fn read_steady_2000mv_pin_is_4000mv_cell_at_80pct() {
        // 2000 mV at the pin → ×2 = 4000 mV cell → 80 %.
        let mut p = init_plugin(ScriptedAdc::steady(2000));
        let PluginResult::Ok(resp) = p.execute(CMD_READ_MV, &[]) else {
            panic!("expected Ok");
        };
        let b = resp.as_slice();
        let cell = u16::from_le_bytes([b[0], b[1]]);
        assert_eq!(cell, 4000);
        assert_eq!(b[2], 80);
    }

    #[test]
    fn read_before_init_errors() {
        let mut p = BatteryAdc::new(ScriptedAdc::steady(2000), 7);
        let PluginResult::Error(e) = p.execute(CMD_READ_MV, &[]) else { panic!() };
        assert_eq!(e.code, ERR_NOT_INIT);
    }

    #[test]
    fn implausible_low_voltage_rejected() {
        // 500 mV pin → 1000 mV cell → below 2500 → implausible.
        let mut p = init_plugin(ScriptedAdc::steady(500));
        let PluginResult::Error(e) = p.execute(CMD_READ_MV, &[]) else { panic!() };
        assert_eq!(e.code, ERR_IMPLAUSIBLE);
    }

    #[test]
    fn wide_spread_rejected_as_unstable() {
        // 16 samples alternating 1500/1650 (pin). Median ~1575 → cell
        // ~3150 (plausible), but spread = 150 pin mV > 100 → unstable.
        let mut s = Vec::new();
        for i in 0..16 {
            s.push(if i % 2 == 0 { 1500 } else { 1650 });
        }
        let mut p = init_plugin(ScriptedAdc::from(s));
        let PluginResult::Error(e) = p.execute(CMD_READ_MV, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNSTABLE);
    }

    #[test]
    fn adc_read_failure_surfaces_err_adc() {
        let mut p = init_plugin(ScriptedAdc::failing_at(3));
        let PluginResult::Error(e) = p.execute(CMD_READ_MV, &[]) else { panic!() };
        assert_eq!(e.code, ERR_ADC);
    }

    #[test]
    fn calibrate_is_declared_but_unimplemented() {
        let mut p = init_plugin(ScriptedAdc::steady(2000));
        let PluginResult::Error(e) = p.execute(CMD_CALIBRATE, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = init_plugin(ScriptedAdc::steady(2000));
        let PluginResult::Error(e) = p.execute(0xAA, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}
