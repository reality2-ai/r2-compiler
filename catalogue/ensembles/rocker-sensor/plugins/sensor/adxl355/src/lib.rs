//! # r2-plugin-sensor-adxl355
//!
//! R2 AOT plugin: Analog Devices ADXL355 precision triaxial MEMS
//! accelerometer over SPI. Provides the generic
//! `ai.reality2.cap.accel.triaxial` capability (plus the
//! `.20bit` refinement) so the sentant calling this plugin is
//! chip-agnostic — same surface as `r2-plugin-sensor-lis2dh` (I²C,
//! 10-bit), swappable per R2-PLUGIN §10.
//!
//! **Reference implementation:** the host-firmware module
//! `r2-workshop/firmware/esp32-s3/devkitc/src/adxl355.rs`. This crate is
//! the R2-PLUGIN §12 conformant refactor of that module — the same
//! register map, command-byte convention, and 20-bit decode, lifted off
//! `esp-idf-svc` and onto the generic `embedded-hal` 1.0 SPI trait.
//!
//! ## Command opcodes
//!
//! Mirrors [`plugin.toml [commands]`]; the compiler plugin emits glue
//! translating a sentant's string command into the matching byte.
//!
//! | Byte | Name         | Input bytes                              | Output bytes              |
//! |------|--------------|------------------------------------------|---------------------------|
//! | 0x01 | init         | `[]` (v0.1 — workshop defaults)          | `[DEVID_AD, DEVID_MST, PARTID]` |
//! | 0x02 | read         | `[]`                                     | `[x: i32 LE | y: i32 LE | z: i32 LE]` (12 bytes) |
//! | 0x03 | read_burst   | `[max_samples: u8]`                      | `[count: u8 | (x,y,z)*N]` |
//! | 0x04 | set_odr      | `[odr_hz: u16 LE]`                       | `[]`                      |
//! | 0x05 | set_range    | `[range_g: u8]`                          | `[]`                      |
//! | 0x06 | set_offset   | `[x: i16 LE | y: i16 LE | z: i16 LE]`    | `[]`                      |
//! | 0x07 | sleep        | `[]`                                     | `[]`                      |
//!
//! `read` returns acceleration in the **256_000-LSB-per-g** convention
//! shared with the LIS2DH wire path (SPEC-R2-WORKSHOP-WIRE §4.1) — at the
//! chip's power-on ±2 g range, 1 g = 256_000 LSB natively, so no rescale
//! is needed (unlike the LIS2DH, whose coarser counts are scaled up).
//!
//! ## SPI abstraction
//!
//! Generic over `embedded_hal::spi::SpiDevice` — the firmware-render step
//! wraps `esp_idf_svc::hal::spi::SpiDeviceDriver` (which implements `eh1`)
//! to satisfy the trait at link time. Per the datasheet §10, each access
//! starts with a command byte `(reg << 1) | R/W` (R/W = 1 read, 0 write);
//! multi-byte reads auto-increment the register address, so a 3-axis
//! sample is a single 10-byte `transfer_in_place`.
//!
//! ## Modes
//!
//! - `aot` — static link into MCU firmware (`no_std`).
//! - `nif` — reserved for a future Linux-SBC NIF build (`false` in
//!   `plugin.toml` today).

// no_std for AOT (MCU) builds; tests run on host where std is available.
#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use embedded_hal::spi::SpiDevice;
use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Command opcode: initialise the sensor — soft-reset, verify IDs, clear standby.
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: read one (x, y, z) sample.
pub const CMD_READ: PluginCommand = 0x02;
/// Command opcode: drain FIFO of up to `max_samples` samples.
pub const CMD_READ_BURST: PluginCommand = 0x03;
/// Command opcode: change ODR (Hz).
pub const CMD_SET_ODR: PluginCommand = 0x04;
/// Command opcode: change ±g range.
pub const CMD_SET_RANGE: PluginCommand = 0x05;
/// Command opcode: write per-axis calibration offsets.
pub const CMD_SET_OFFSET: PluginCommand = 0x06;
/// Command opcode: enter low-power / standby mode.
pub const CMD_SLEEP: PluginCommand = 0x07;

/// Error code: input byte length did not match the command's required layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error code: SPI transaction failed (bus error).
pub const ERR_SPI_BUS: u8 = 0x02;
/// Error code: identification registers did not match an ADXL355.
pub const ERR_WHO_AM_I: u8 = 0x03;
/// Error code: invalid ±g range (must be 2, 4, or 8).
pub const ERR_BAD_RANGE: u8 = 0x04;
/// Error code: invalid ODR.
pub const ERR_BAD_ODR: u8 = 0x05;
/// Error code: command byte was not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

// ── ADXL355 register map (datasheet §11) ──────────────────────────────

/// DEVID_AD register — first of three identification registers; an
/// auto-incrementing read returns DEVID_AD, DEVID_MST, PARTID.
const REG_DEVID_AD: u8 = 0x00;
/// XDATA3 — first sample register; auto-increment runs through ZDATA1 at 0x10.
const REG_XDATA3: u8 = 0x08;
/// POWER_CTL — bit 0 = standby (1) / measurement (0).
const REG_POWER_CTL: u8 = 0x2D;
/// RESET — write `RESET_CODE` to force power-on defaults.
const REG_RESET: u8 = 0x2F;

/// Expected DEVID_AD (Analog Devices ID).
pub const EXPECTED_DEVID_AD: u8 = 0xAD;
/// Expected DEVID_MST (MEMS family ID).
pub const EXPECTED_DEVID_MST: u8 = 0x1D;
/// Expected PARTID (ADXL355 part).
pub const EXPECTED_PARTID: u8 = 0xED;

/// POWER_CTL value for measurement mode (bit 0 = 0).
const POWER_CTL_MEASURE: u8 = 0x00;
/// Code written to REG_RESET to trigger a soft reset (datasheet §11).
const RESET_CODE: u8 = 0x52;

/// SPI command-byte R/W bit: 1 = read.
const RW_READ: u8 = 0x01;

/// ADXL355 plugin.
///
/// Generic over `S: SpiDevice` so the same plugin works against any
/// `embedded-hal` 1.0 SPI device — the firmware build wraps the
/// platform's `SpiDeviceDriver` (with its own chip-select) to satisfy the
/// trait. The plugin owns the SPI device for its lifetime.
pub struct Adxl355<S: SpiDevice> {
    dev: S,
    initialised: bool,
    id: PluginId,
}

impl<S: SpiDevice> Adxl355<S> {
    /// Construct an un-initialised plugin instance bound to `id`. Call
    /// `Plugin::init` (via the engine) — or invoke `CMD_INIT` — before
    /// issuing reads.
    pub const fn new(dev: S, id: PluginId) -> Self {
        Self { dev, initialised: false, id }
    }

    /// Write one register. The R/W bit is 0 (write) — note the ADXL355
    /// puts R/W in the *low* bit of the command byte, not the high bit.
    fn write_reg(&mut self, reg: u8, val: u8) -> Result<(), PluginError> {
        let mut buf = [(reg << 1) & !RW_READ, val];
        self.dev
            .transfer_in_place(&mut buf)
            .map_err(|_| PluginError::new(ERR_SPI_BUS, "adxl355 write_reg"))
    }

    /// Read the three identification registers in one auto-incrementing
    /// transaction.
    fn read_ids(&mut self) -> Result<(u8, u8, u8), PluginError> {
        let mut buf = [(REG_DEVID_AD << 1) | RW_READ, 0, 0, 0];
        self.dev
            .transfer_in_place(&mut buf)
            .map_err(|_| PluginError::new(ERR_SPI_BUS, "adxl355 read_ids"))?;
        Ok((buf[1], buf[2], buf[3]))
    }

    /// Read X / Y / Z as signed 20-bit LSB values in one transaction.
    fn read_axes(&mut self) -> Result<(i32, i32, i32), PluginError> {
        // 1 command byte + 9 data bytes (3 axes × 3 bytes each).
        let mut buf = [0u8; 10];
        buf[0] = (REG_XDATA3 << 1) | RW_READ;
        self.dev
            .transfer_in_place(&mut buf)
            .map_err(|_| PluginError::new(ERR_SPI_BUS, "adxl355 read_axes"))?;
        Ok((
            decode_20bit_signed(&buf[1..4]),
            decode_20bit_signed(&buf[4..7]),
            decode_20bit_signed(&buf[7..10]),
        ))
    }

    // ── Command handlers ──────────────────────────────────────────────

    fn op_init(&mut self, _data: &[u8]) -> PluginResult {
        // Soft-reset to clear any partial-transaction state from a bouncy
        // boot (the workshop module's cold-boot defence), verify the three
        // ID registers, then clear standby into measurement mode. The
        // workshop's inter-step `sleep`s are esp-idf boot-race mitigations
        // handled by the firmware-render layer, not this pure driver.
        if let Err(e) = self.write_reg(REG_RESET, RESET_CODE) {
            return PluginResult::Error(e);
        }
        let (ad, mst, part) = match self.read_ids() {
            Ok(ids) => ids,
            Err(e) => return PluginResult::Error(e),
        };
        if ad != EXPECTED_DEVID_AD || mst != EXPECTED_DEVID_MST || part != EXPECTED_PARTID {
            return PluginResult::Error(PluginError::new(
                ERR_WHO_AM_I,
                "adxl355 ID mismatch (expected 0xAD/0x1D/0xED)",
            ));
        }
        if let Err(e) = self.write_reg(REG_POWER_CTL, POWER_CTL_MEASURE) {
            return PluginResult::Error(e);
        }
        self.initialised = true;
        PluginResult::Ok(PluginResponse::with_data(&[ad, mst, part]))
    }

    fn op_read(&mut self, _data: &[u8]) -> PluginResult {
        if !self.initialised {
            return PluginResult::Error(PluginError::new(
                ERR_SPI_BUS,
                "adxl355: read before init",
            ));
        }
        match self.read_axes() {
            Ok((x, y, z)) => {
                let mut out = [0u8; 12];
                out[0..4].copy_from_slice(&x.to_le_bytes());
                out[4..8].copy_from_slice(&y.to_le_bytes());
                out[8..12].copy_from_slice(&z.to_le_bytes());
                PluginResult::Ok(PluginResponse::with_data(&out))
            }
            Err(e) => PluginResult::Error(e),
        }
    }
}

impl<S: SpiDevice> Plugin for Adxl355<S> {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => self.op_init(data),
            CMD_READ => self.op_read(data),
            // TODO Phase 1.4-source-extended: implement read_burst (FIFO),
            // set_odr / set_range / set_offset / sleep. The reference
            // module exposes only init + read today; the rest are
            // R2-PLUGIN §12 commands declared ahead of full driver parity.
            CMD_READ_BURST | CMD_SET_ODR | CMD_SET_RANGE | CMD_SET_OFFSET | CMD_SLEEP => {
                PluginResult::Error(PluginError::new(
                    ERR_UNKNOWN_COMMAND,
                    "adxl355: command declared in plugin.toml but not yet implemented",
                ))
            }
            _ => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND,
                "adxl355: unknown command byte",
            )),
        }
    }

    fn name(&self) -> &str {
        "sensor/adxl355"
    }

    fn id(&self) -> PluginId {
        self.id
    }

    fn init(&mut self) -> PluginResult {
        self.op_init(&[])
    }
}

/// Decode a 20-bit signed integer stored left-aligned across 3 bytes
/// (big-endian, MSB-first; the low 4 bits of the third byte are reserved
/// zeros per datasheet §11). Returns a sign-extended `i32`.
///
/// At the power-on ±2 g range, 1 g = 256_000 LSB (datasheet §6) — the
/// same numeric convention the LIS2DH driver rescales to, so downstream
/// code stays sensor-agnostic.
pub fn decode_20bit_signed(b: &[u8]) -> i32 {
    let raw24 = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
    let raw20 = raw24 >> 4;
    let sign_bit = 1u32 << 19;
    if (raw20 & sign_bit) != 0 {
        (raw20 | 0xFFF0_0000) as i32
    } else {
        raw20 as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction as SpiTrans};

    // The eh1 `Mock` as a `SpiDevice` frames each device op with
    // transaction_start / transaction_end; build expectations accordingly.
    fn write_reg_seq(reg: u8, val: u8) -> [SpiTrans<u8>; 3] {
        let cmd = (reg << 1) & !RW_READ;
        [
            SpiTrans::transaction_start(),
            SpiTrans::transfer_in_place(vec![cmd, val], vec![0x00, 0x00]),
            SpiTrans::transaction_end(),
        ]
    }

    fn read_ids_seq(ad: u8, mst: u8, part: u8) -> [SpiTrans<u8>; 3] {
        let cmd = (REG_DEVID_AD << 1) | RW_READ;
        [
            SpiTrans::transaction_start(),
            SpiTrans::transfer_in_place(vec![cmd, 0, 0, 0], vec![0x00, ad, mst, part]),
            SpiTrans::transaction_end(),
        ]
    }

    #[test]
    fn decode_zero() {
        assert_eq!(decode_20bit_signed(&[0x00, 0x00, 0x00]), 0);
    }

    #[test]
    fn decode_positive_max() {
        assert_eq!(decode_20bit_signed(&[0x7F, 0xFF, 0xF0]), 524_287);
    }

    #[test]
    fn decode_negative_one() {
        assert_eq!(decode_20bit_signed(&[0xFF, 0xFF, 0xF0]), -1);
    }

    #[test]
    fn decode_one_g_at_2g_range() {
        // 1 g at ±2 g = 256_000 LSB = 0x3E800 (20-bit) → left-aligned
        // into the 24-bit field as (0x3E800 << 4) = 0x3E_8000 → big-endian
        // bytes [0x3E, 0x80, 0x00]. (NB: r2-workshop's own test uses
        // [0x03, 0xE8, 0x00] here, which actually decodes to 16_000 — a
        // latent error in that test's example bytes, not the decoder.)
        assert_eq!(decode_20bit_signed(&[0x3E, 0x80, 0x00]), 256_000);
    }

    #[test]
    fn init_resets_verifies_ids_and_clears_standby() {
        let mut expectations = Vec::new();
        expectations.extend(write_reg_seq(REG_RESET, RESET_CODE));
        expectations.extend(read_ids_seq(EXPECTED_DEVID_AD, EXPECTED_DEVID_MST, EXPECTED_PARTID));
        expectations.extend(write_reg_seq(REG_POWER_CTL, POWER_CTL_MEASURE));

        let mut spi = SpiMock::new(&expectations);
        let mut plugin = Adxl355::new(spi.clone(), 7);

        let result = plugin.execute(CMD_INIT, &[]);
        let PluginResult::Ok(resp) = result else {
            panic!("expected Ok from CMD_INIT, got {result:?}");
        };
        assert_eq!(resp.as_slice(), &[EXPECTED_DEVID_AD, EXPECTED_DEVID_MST, EXPECTED_PARTID]);
        drop(plugin);
        spi.done();
    }

    #[test]
    fn init_rejects_wrong_ids() {
        let mut expectations = Vec::new();
        expectations.extend(write_reg_seq(REG_RESET, RESET_CODE));
        expectations.extend(read_ids_seq(0x00, 0x00, 0x00)); // not an ADXL355
        // No POWER_CTL write — init bails after the ID check.

        let mut spi = SpiMock::new(&expectations);
        let mut plugin = Adxl355::new(spi.clone(), 7);

        let result = plugin.execute(CMD_INIT, &[]);
        let PluginResult::Error(err) = result else {
            panic!("expected Error from CMD_INIT with bad IDs, got {result:?}");
        };
        assert_eq!(err.code, ERR_WHO_AM_I);
        drop(plugin);
        spi.done();
    }

    #[test]
    fn read_returns_x_y_z_in_lsb_per_g_convention() {
        // 1 g on x, 0 on y, -1 g on z (20-bit left-aligned big-endian).
        // +256_000 = 0x3E800 << 4 = 0x3E_8000 → [0x3E, 0x80, 0x00].
        // -256_000 = 0xC1800 (20-bit two's complement) << 4 = 0xC1_8000
        //            → [0xC1, 0x80, 0x00].
        let read_cmd = (REG_XDATA3 << 1) | RW_READ;
        let resp = vec![
            0x00, // echoed command-byte slot
            0x3E, 0x80, 0x00, // x = +256_000
            0x00, 0x00, 0x00, // y = 0
            0xC1, 0x80, 0x00, // z = -256_000
        ];

        let mut expectations = Vec::new();
        expectations.extend(write_reg_seq(REG_RESET, RESET_CODE));
        expectations.extend(read_ids_seq(EXPECTED_DEVID_AD, EXPECTED_DEVID_MST, EXPECTED_PARTID));
        expectations.extend(write_reg_seq(REG_POWER_CTL, POWER_CTL_MEASURE));
        expectations.push(SpiTrans::transaction_start());
        expectations.push(SpiTrans::transfer_in_place(
            vec![read_cmd, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            resp,
        ));
        expectations.push(SpiTrans::transaction_end());

        let mut spi = SpiMock::new(&expectations);
        let mut plugin = Adxl355::new(spi.clone(), 7);

        plugin.execute(CMD_INIT, &[]);
        let result = plugin.execute(CMD_READ, &[]);
        let PluginResult::Ok(resp) = result else {
            panic!("expected Ok from CMD_READ, got {result:?}");
        };
        let bytes = resp.as_slice();
        let x = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let y = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let z = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
        assert_eq!(x, 256_000);
        assert_eq!(y, 0);
        assert_eq!(z, -256_000);
        drop(plugin);
        spi.done();
    }

    #[test]
    fn read_before_init_errors() {
        let mut spi = SpiMock::new(&[]);
        let mut plugin = Adxl355::new(spi.clone(), 7);
        let result = plugin.execute(CMD_READ, &[]);
        let PluginResult::Error(err) = result else {
            panic!("expected Error from CMD_READ before init, got {result:?}");
        };
        assert_eq!(err.code, ERR_SPI_BUS);
        drop(plugin);
        spi.done();
    }

    #[test]
    fn unknown_command_errors_with_known_code() {
        let mut spi = SpiMock::new(&[]);
        let mut plugin = Adxl355::new(spi.clone(), 7);
        let PluginResult::Error(err) = plugin.execute(0xAA, &[]) else { panic!() };
        assert_eq!(err.code, ERR_UNKNOWN_COMMAND);
        drop(plugin);
        spi.done();
    }

    #[test]
    fn declared_but_unimplemented_commands_report_unknown_code() {
        let mut spi = SpiMock::new(&[]);
        let mut plugin = Adxl355::new(spi.clone(), 7);
        for cmd in [CMD_READ_BURST, CMD_SET_ODR, CMD_SET_RANGE, CMD_SET_OFFSET, CMD_SLEEP] {
            let PluginResult::Error(err) = plugin.execute(cmd, &[]) else {
                panic!("command 0x{cmd:02X} should still be unimplemented");
            };
            assert_eq!(err.code, ERR_UNKNOWN_COMMAND);
        }
        drop(plugin);
        spi.done();
    }
}
