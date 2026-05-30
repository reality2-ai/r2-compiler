//! # r2-plugin-sensor-lis2dh
//!
//! R2 AOT plugin: STMicroelectronics LIS2DH 3-axis MEMS accelerometer over
//! I²C. Provides the generic `ai.reality2.cap.accel.triaxial` capability so
//! the sentant calling this plugin is chip-agnostic — same surface as the
//! `r2-plugin-sensor-adxl355` (SPI, higher precision).
//!
//! **Reference implementation:** the host-firmware module
//! `r2-workshop/firmware/esp32-c6/dfr1117/src/lis2dh.rs`. This crate is
//! the R2-PLUGIN §12 conformant refactor of that module.
//!
//! ## Command opcodes
//!
//! Mapping declared in [`plugin.toml [commands]`] and mirrored as
//! constants below. The compiler plugin emits glue that translates a
//! sentant's string command into the matching byte.
//!
//! | Byte | Name         | Input bytes                              | Output bytes              |
//! |------|--------------|------------------------------------------|---------------------------|
//! | 0x01 | init         | `[odr_hz: u16 LE | range_g: u8 | …]`     | `[WHO_AM_I: u8]`          |
//! | 0x02 | read         | `[]`                                     | `[x: i32 LE | y: i32 LE | z: i32 LE]` (12 bytes) |
//! | 0x03 | read_burst   | `[max_samples: u8]`                      | `[count: u8 | (x,y,z)*N]` |
//! | 0x04 | set_odr      | `[odr_hz: u16 LE]`                       | `[]`                      |
//! | 0x05 | set_range    | `[range_g: u8]`                          | `[]`                      |
//! | 0x06 | set_offset   | `[x: i16 LE | y: i16 LE | z: i16 LE]`    | `[]`                      |
//! | 0x07 | sleep        | `[]`                                     | `[]`                      |
//!
//! `read` returns acceleration in the **256_000-LSB-per-g** convention
//! shared with the ADXL355 wire path (see r2-workshop SPEC-R2-WORKSHOP-WIRE
//! §4.1). The LIS2DH's native counts are rescaled inside this plugin so
//! its coarser 10-bit resolution shows up as honest quantisation (steps
//! of 256 LSB at ±2 g HR) rather than a wrong scale.
//!
//! ## Modes
//!
//! - `aot` — static link into MCU firmware (`no_std`).
//! - `nif` — reserved for future Linux-SBC NIF build (currently `false` in
//!   `plugin.toml`).
//!
//! ## I²C abstraction
//!
//! The plugin is generic over `embedded_hal::i2c::I2c`. The firmware-render
//! step wraps `esp_idf_svc::hal::i2c::I2cDriver` (which implements `eh1`)
//! to satisfy the trait at link time. This keeps the plugin truly
//! `no_std` (no esp-idf-svc dep here).

// Crate is no_std for AOT (MCU) builds. Tests run on host where std is
// available — `test` cfg keeps std in scope for the integration mocks
// (embedded-hal-mock + vec! macros).
#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use embedded_hal::i2c::I2c;
use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Command opcode: initialise the sensor — probe WHO_AM_I, configure ODR + range + offset.
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: read one (x, y, z) sample.
pub const CMD_READ: PluginCommand = 0x02;
/// Command opcode: drain FIFO of up to `max_samples` (≤ 32) samples.
pub const CMD_READ_BURST: PluginCommand = 0x03;
/// Command opcode: change ODR (Hz).
pub const CMD_SET_ODR: PluginCommand = 0x04;
/// Command opcode: change ±g range.
pub const CMD_SET_RANGE: PluginCommand = 0x05;
/// Command opcode: write per-axis calibration offsets.
pub const CMD_SET_OFFSET: PluginCommand = 0x06;
/// Command opcode: enter low-power mode.
pub const CMD_SLEEP: PluginCommand = 0x07;

/// Error code: input byte length did not match the command's required layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error code: I²C transaction failed (no ACK, bus error, etc.).
pub const ERR_I2C_BUS: u8 = 0x02;
/// Error code: WHO_AM_I returned a value ≠ 0x33 (chip is not a LIS2DH).
pub const ERR_WHO_AM_I: u8 = 0x03;
/// Error code: invalid ±g range (must be 2, 4, 8, or 16).
pub const ERR_BAD_RANGE: u8 = 0x04;
/// Error code: invalid ODR (must be one of the LIS2DH-supported values).
pub const ERR_BAD_ODR: u8 = 0x05;
/// Error code: command byte was not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

// ── LIS2DH register map (datasheet §8) ────────────────────────────────

/// WHO_AM_I register address.
const REG_WHO_AM_I: u8 = 0x0F;
/// Expected WHO_AM_I value for the LIS2DH die.
const EXPECTED_WHO_AM_I: u8 = 0x33;

/// CTRL_REG1 — ODR (bits 7-4), LPen (bit 3), Z/Y/X enable (bits 2-0).
const REG_CTRL1: u8 = 0x20;
/// CTRL_REG4 — BDU (bit 7), FS (bits 5-4), HR (bit 3).
const REG_CTRL4: u8 = 0x23;
/// OUT_X_L — first output register; auto-increments through OUT_Z_H with sub-addr MSB set.
const REG_OUT_X_L: u8 = 0x28;
/// Sub-address MSB for auto-increment burst reads.
const AUTO_INC: u8 = 0x80;

/// CTRL1 = 0b0111_0111: ODR=400 Hz (0111), LPen=0 (HR/normal), Z/Y/X enabled.
const CTRL1_400HZ_XYZ: u8 = 0x77;
/// CTRL4 = 0b1000_1000: BDU=1, FS=±2 g (00), HR=1 (high-resolution, 12-bit).
const CTRL4_BDU_HR_2G: u8 = 0x88;

/// Candidate 7-bit I²C addresses — LIS2DH SA0 strap selects 0x18 or 0x19.
const ADDR_CANDIDATES: [u8; 2] = [0x18, 0x19];

/// At ±2 g HR the 12-bit sample is 1 mg/digit; × 256 maps mg →
/// the 256_000-LSB/g convention shared with the ADXL355 wire path.
const LSB_PER_DIGIT_2G_HR: i32 = 256;

/// LIS2DH plugin.
///
/// Generic over `B: I2c` so the same plugin works against any
/// `embedded_hal` 1.0 I²C bus — the firmware build wraps the platform's
/// `I2cDriver` to satisfy the trait.
///
/// The plugin owns the I²C bus reference for its lifetime. On constrained
/// devices that's a single peripheral, so this is appropriate.
pub struct Lis2dh<B: I2c> {
    bus: B,
    addr: u8,
    initialised: bool,
    id: PluginId,
}

impl<B: I2c> Lis2dh<B> {
    /// Construct an un-initialised plugin instance bound to `id`. Call
    /// `Plugin::init` (via the engine) — or invoke command `CMD_INIT` —
    /// before issuing reads.
    pub const fn new(bus: B, id: PluginId) -> Self {
        Self {
            bus,
            addr: ADDR_CANDIDATES[0],
            initialised: false,
            id,
        }
    }

    /// Probe both candidate 7-bit addresses for the expected `WHO_AM_I`.
    /// Returns the discovered address on success.
    fn probe(&mut self) -> Result<u8, PluginError> {
        for &candidate in &ADDR_CANDIDATES {
            let mut buf = [0u8; 1];
            // write_read: write the register address, then read its value.
            match self.bus.write_read(candidate, &[REG_WHO_AM_I], &mut buf) {
                Ok(()) if buf[0] == EXPECTED_WHO_AM_I => return Ok(candidate),
                Ok(()) => continue,           // wrong WHO_AM_I, try next addr
                Err(_) => continue,           // no ACK at this address, try next
            }
        }
        Err(PluginError::new(
            ERR_WHO_AM_I,
            "LIS2DH not found at 0x18 or 0x19 (WHO_AM_I mismatch)",
        ))
    }

    fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), PluginError> {
        self.bus
            .write(self.addr, &[reg, value])
            .map_err(|_| PluginError::new(ERR_I2C_BUS, "LIS2DH write_reg"))
    }

    fn read_axes_raw(&mut self) -> Result<(i32, i32, i32), PluginError> {
        let mut buf = [0u8; 6];
        self.bus
            .write_read(self.addr, &[REG_OUT_X_L | AUTO_INC], &mut buf)
            .map_err(|_| PluginError::new(ERR_I2C_BUS, "LIS2DH burst read"))?;
        Ok((decode_axis(buf[0], buf[1]), decode_axis(buf[2], buf[3]), decode_axis(buf[4], buf[5])))
    }

    // ── Command handlers ──────────────────────────────────────────────

    fn op_init(&mut self, _data: &[u8]) -> PluginResult {
        // Probe + configure for HR mode, ±2 g, 400 Hz, all axes, BDU.
        // (Future revision: parse data:[odr_hz, range_g, …] from the
        // input bytes and configure dynamically. v0.1 takes the same
        // defaults the r2-workshop firmware uses.)
        let addr = match self.probe() {
            Ok(a) => a,
            Err(e) => return PluginResult::Error(e),
        };
        self.addr = addr;
        if let Err(e) = self.write_reg(REG_CTRL1, CTRL1_400HZ_XYZ) {
            return PluginResult::Error(e);
        }
        if let Err(e) = self.write_reg(REG_CTRL4, CTRL4_BDU_HR_2G) {
            return PluginResult::Error(e);
        }
        self.initialised = true;
        PluginResult::Ok(PluginResponse::with_data(&[EXPECTED_WHO_AM_I]))
    }

    fn op_read(&mut self, _data: &[u8]) -> PluginResult {
        if !self.initialised {
            return PluginResult::Error(PluginError::new(
                ERR_I2C_BUS,
                "lis2dh: read before init",
            ));
        }
        match self.read_axes_raw() {
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

impl<B: I2c> Plugin for Lis2dh<B> {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => self.op_init(data),
            CMD_READ => self.op_read(data),
            // TODO Phase 1.4-source-extended: implement read_burst (FIFO),
            // set_odr / set_range / set_offset / sleep. The reference
            // module in r2-workshop only exposes init + read at v0.2 of
            // its firmware; the rest are R2-PLUGIN §12 commands declared
            // ahead of full driver feature parity.
            CMD_READ_BURST | CMD_SET_ODR | CMD_SET_RANGE | CMD_SET_OFFSET | CMD_SLEEP => {
                PluginResult::Error(PluginError::new(
                    ERR_UNKNOWN_COMMAND,
                    "lis2dh: command declared in plugin.toml but not yet implemented",
                ))
            }
            _ => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND,
                "lis2dh: unknown command byte",
            )),
        }
    }

    fn name(&self) -> &str {
        "sensor/lis2dh"
    }

    fn id(&self) -> PluginId {
        self.id
    }

    fn init(&mut self) -> PluginResult {
        self.op_init(&[])
    }
}

/// Decode one axis from its low+high LIS2DH OUT register bytes.
///
/// OUT registers are 16-bit left-justified; in HR mode the significant
/// 12 bits are `raw_i16 >> 4` (arithmetic, sign-preserving). Returned
/// value is rescaled to the **256_000-LSB-per-g** convention shared with
/// the ADXL355 wire path.
///
/// At ±2 g HR: 1 g = 1000 digits → 1000 × 256 = 256_000 LSB. The same
/// physical acceleration produces the same numeric value here as it does
/// in the ADXL355 driver — downstream code stays sensor-agnostic.
pub fn decode_axis(lo: u8, hi: u8) -> i32 {
    let raw = i16::from_le_bytes([lo, hi]);
    let digit12 = (raw >> 4) as i32;
    digit12 * LSB_PER_DIGIT_2G_HR
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as I2cTrans};

    #[test]
    fn decode_zero_is_zero() {
        assert_eq!(decode_axis(0x00, 0x00), 0);
    }

    #[test]
    fn decode_one_g_at_2g_hr() {
        // 1 g at ±2 g HR = 1000 digits; left-justified 16-bit = 1000 << 4.
        let raw = (1000_i16) << 4;
        let [lo, hi] = raw.to_le_bytes();
        assert_eq!(decode_axis(lo, hi), 256_000);
    }

    #[test]
    fn decode_negative_one_g_at_2g_hr() {
        let raw = (-1000_i16) << 4;
        let [lo, hi] = raw.to_le_bytes();
        assert_eq!(decode_axis(lo, hi), -256_000);
    }

    #[test]
    fn init_probes_addresses_and_configures_registers() {
        // First probe at 0x18 returns the right WHO_AM_I.
        // Then we expect CTRL1 + CTRL4 writes at the discovered address.
        let expectations = [
            I2cTrans::write_read(0x18, vec![REG_WHO_AM_I], vec![EXPECTED_WHO_AM_I]),
            I2cTrans::write(0x18, vec![REG_CTRL1, CTRL1_400HZ_XYZ]),
            I2cTrans::write(0x18, vec![REG_CTRL4, CTRL4_BDU_HR_2G]),
        ];
        let mut bus = I2cMock::new(&expectations);
        let mut plugin = Lis2dh::new(bus.clone(), 7);

        let result = plugin.execute(CMD_INIT, &[]);
        let PluginResult::Ok(resp) = result else {
            panic!("expected Ok from CMD_INIT, got {result:?}");
        };
        assert_eq!(resp.as_slice(), &[EXPECTED_WHO_AM_I]);
        bus.done();
    }

    #[test]
    fn init_falls_through_to_0x19_when_0x18_wrong_who_am_i() {
        let expectations = [
            // 0x18 ACKs but returns wrong WHO_AM_I.
            I2cTrans::write_read(0x18, vec![REG_WHO_AM_I], vec![0x00]),
            // 0x19 returns the right one.
            I2cTrans::write_read(0x19, vec![REG_WHO_AM_I], vec![EXPECTED_WHO_AM_I]),
            // Then configure at 0x19.
            I2cTrans::write(0x19, vec![REG_CTRL1, CTRL1_400HZ_XYZ]),
            I2cTrans::write(0x19, vec![REG_CTRL4, CTRL4_BDU_HR_2G]),
        ];
        let mut bus = I2cMock::new(&expectations);
        let mut plugin = Lis2dh::new(bus.clone(), 7);

        let result = plugin.execute(CMD_INIT, &[]);
        assert!(matches!(result, PluginResult::Ok(_)));
        bus.done();
    }

    #[test]
    fn read_returns_x_y_z_in_lsb_per_g_convention() {
        // Pre-configure for 1 g on x, 0 on y, -1 g on z.
        let one_g_le = ((1000_i16) << 4).to_le_bytes();
        let zero_le = [0u8, 0u8];
        let neg_one_g_le = ((-1000_i16) << 4).to_le_bytes();
        let payload = [
            one_g_le[0], one_g_le[1],
            zero_le[0], zero_le[1],
            neg_one_g_le[0], neg_one_g_le[1],
        ];

        let expectations = [
            // init
            I2cTrans::write_read(0x18, vec![REG_WHO_AM_I], vec![EXPECTED_WHO_AM_I]),
            I2cTrans::write(0x18, vec![REG_CTRL1, CTRL1_400HZ_XYZ]),
            I2cTrans::write(0x18, vec![REG_CTRL4, CTRL4_BDU_HR_2G]),
            // read
            I2cTrans::write_read(
                0x18,
                vec![REG_OUT_X_L | AUTO_INC],
                payload.to_vec(),
            ),
        ];
        let mut bus = I2cMock::new(&expectations);
        let mut plugin = Lis2dh::new(bus.clone(), 7);

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
        bus.done();
    }

    #[test]
    fn read_before_init_errors() {
        let mut bus = I2cMock::new(&[]);
        let mut plugin = Lis2dh::new(bus.clone(), 7);

        let result = plugin.execute(CMD_READ, &[]);
        let PluginResult::Error(err) = result else {
            panic!("expected Error from CMD_READ before init, got {result:?}");
        };
        assert_eq!(err.code, ERR_I2C_BUS);
        // No bus transactions occurred — drop the plugin so it releases
        // its clone of the mock, then assert the mock saw nothing.
        drop(plugin);
        bus.done();
    }

    #[test]
    fn unknown_command_errors_with_known_code() {
        let mut bus = I2cMock::new(&[]);
        let mut plugin = Lis2dh::new(bus.clone(), 7);
        let result = plugin.execute(0xAA, &[]);
        let PluginResult::Error(err) = result else { panic!() };
        assert_eq!(err.code, ERR_UNKNOWN_COMMAND);
        drop(plugin);
        bus.done();
    }

    #[test]
    fn declared_but_unimplemented_commands_report_unknown_code() {
        let mut bus = I2cMock::new(&[]);
        let mut plugin = Lis2dh::new(bus.clone(), 7);
        for cmd in [CMD_READ_BURST, CMD_SET_ODR, CMD_SET_RANGE, CMD_SET_OFFSET, CMD_SLEEP] {
            let result = plugin.execute(cmd, &[]);
            let PluginResult::Error(err) = result else {
                panic!("command 0x{cmd:02X} should still be unimplemented");
            };
            assert_eq!(err.code, ERR_UNKNOWN_COMMAND);
        }
        drop(plugin);
        bus.done();
    }
}
