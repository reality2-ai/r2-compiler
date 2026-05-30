//! # r2-plugin-time-clock
//!
//! R2 **always-on core** plugin: monotonic + offset wall-clock.
//!
//! Every event-emitting sentant timestamps its output (R2-WIRE frames,
//! capture records, status events, sync replies). Without a clock, the
//! wire format breaks. The CAPABILITY is non-negotiable; per-platform
//! IMPLEMENTATION varies — hence the trait.
//!
//! ## Two clocks, one source
//!
//! - `monotonic_ms()` — platform tick counter since boot. Strictly
//!   increasing, no jumps. Used for relative timestamps within a
//!   measurement session.
//! - `now_ms()` = `monotonic_ms() + offset_ms` — wall-clock-relative
//!   timestamp. The offset is set by the `Sync` sentant via Cristian's
//!   algorithm; this plugin holds it in memory. Persistence (e.g. to
//!   NVS) is the consuming sentant's responsibility — the plugin's job
//!   is just the read/write surface.
//!
//! ## Command opcodes
//!
//! | Byte | Name           | Input                | Output                       |
//! |------|----------------|----------------------|------------------------------|
//! | 0x01 | init           | empty                | empty                        |
//! | 0x02 | monotonic_ms   | empty                | `[ms: u64 LE]` (8 B)         |
//! | 0x03 | now_ms         | empty                | `[ms: u64 LE]` (8 B)         |
//! | 0x04 | set_offset     | `[offset_ms: i64 LE]`| empty                        |
//! | 0x05 | get_offset     | empty                | `[offset_ms: i64 LE]` (8 B)  |
//!
//! ## Modes
//!
//! - `aot` — static link into MCU firmware (`no_std`). Firmware-render
//!   step plugs in an ESP-IDF `Clock` wrapping `esp_timer_get_time()`.
//! - `nif` — reserved for future Linux-NIF / browser-WASM hives that
//!   wrap their platform's monotonic source.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Command opcode: initialise the clock (one-shot; idempotent).
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: read `monotonic_ms()` — platform tick counter since boot.
pub const CMD_MONOTONIC_MS: PluginCommand = 0x02;
/// Command opcode: read `now_ms()` — `monotonic_ms() + offset_ms`.
pub const CMD_NOW_MS: PluginCommand = 0x03;
/// Command opcode: set the wall-clock offset (typically called by the Sync sentant after Cristian's algorithm).
pub const CMD_SET_OFFSET: PluginCommand = 0x04;
/// Command opcode: read the current wall-clock offset.
pub const CMD_GET_OFFSET: PluginCommand = 0x05;

/// Error: input byte length did not match the command's required layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error: command byte was not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Platform monotonic-tick source.
///
/// The firmware-render step plugs in the carrier-appropriate impl
/// (ESP-IDF `esp_timer_get_time()` for the rocker sensors; Linux
/// `CLOCK_MONOTONIC` for SBC hives; `performance.now()` for browser
/// WASM hives). The plugin source is generic so the same code compiles
/// for all targets.
pub trait Clock {
    /// Return the current monotonic tick in milliseconds since some
    /// arbitrary epoch (typically system boot). MUST be monotonically
    /// non-decreasing across successive calls within one process.
    fn monotonic_ms(&self) -> u64;
}

/// Clock plugin parameterised over a `Clock` source.
pub struct ClockPlugin<C: Clock> {
    source: C,
    offset_ms: i64,
    initialised: bool,
    id: PluginId,
}

impl<C: Clock> ClockPlugin<C> {
    /// Construct a new instance wrapping `source`. The clock is
    /// pre-initialised with an offset of zero; call `CMD_SET_OFFSET`
    /// after a successful Sync handshake to update.
    pub const fn new(source: C, id: PluginId) -> Self {
        Self { source, offset_ms: 0, initialised: false, id }
    }

    /// Returns the in-memory offset. Useful for tests that need to
    /// inspect plugin state without going through the byte protocol.
    pub fn offset_ms(&self) -> i64 { self.offset_ms }

    fn op_init(&mut self, _data: &[u8]) -> PluginResult {
        self.initialised = true;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn op_monotonic_ms(&self) -> PluginResult {
        let ms = self.source.monotonic_ms();
        PluginResult::Ok(PluginResponse::with_data(&ms.to_le_bytes()))
    }

    fn op_now_ms(&self) -> PluginResult {
        // Saturating arithmetic — a negative offset that would underflow
        // u64 clamps to 0 rather than wrapping.
        let mono = self.source.monotonic_ms();
        let now = (mono as i128 + self.offset_ms as i128).max(0) as u64;
        PluginResult::Ok(PluginResponse::with_data(&now.to_le_bytes()))
    }

    fn op_set_offset(&mut self, data: &[u8]) -> PluginResult {
        if data.len() != 8 {
            return PluginResult::Error(PluginError::new(ERR_BAD_LENGTH, "set_offset: need 8 bytes (i64 LE)"));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(data);
        self.offset_ms = i64::from_le_bytes(buf);
        PluginResult::Ok(PluginResponse::empty())
    }

    fn op_get_offset(&self) -> PluginResult {
        PluginResult::Ok(PluginResponse::with_data(&self.offset_ms.to_le_bytes()))
    }
}

impl<C: Clock> Plugin for ClockPlugin<C> {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => self.op_init(data),
            CMD_MONOTONIC_MS => self.op_monotonic_ms(),
            CMD_NOW_MS => self.op_now_ms(),
            CMD_SET_OFFSET => self.op_set_offset(data),
            CMD_GET_OFFSET => self.op_get_offset(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command byte")),
        }
    }

    fn name(&self) -> &str { "time/clock" }
    fn id(&self) -> PluginId { self.id }

    fn init(&mut self) -> PluginResult {
        self.op_init(&[])
    }
}

// ── In-memory clock — for tests + host-side usage ────────────────────

/// In-memory `Clock` impl backed by a mutable internal tick. Tests can
/// advance time explicitly via `set` / `advance`. Available with `std`
/// or under `#[cfg(test)]`.
#[cfg(any(feature = "std", test))]
pub mod mem {
    use super::*;
    use std::cell::Cell;

    /// Deterministic clock for tests.
    pub struct InMemoryClock {
        ticks: Cell<u64>,
    }

    impl Default for InMemoryClock {
        fn default() -> Self { Self::new() }
    }

    impl InMemoryClock {
        /// Construct a clock at tick zero.
        pub const fn new() -> Self {
            Self { ticks: Cell::new(0) }
        }

        /// Construct a clock starting at `start_ms`.
        pub const fn at(start_ms: u64) -> Self {
            Self { ticks: Cell::new(start_ms) }
        }

        /// Advance the clock by `delta_ms`.
        pub fn advance(&self, delta_ms: u64) {
            self.ticks.set(self.ticks.get().saturating_add(delta_ms));
        }

        /// Set the clock to an exact tick.
        pub fn set(&self, ms: u64) {
            self.ticks.set(ms);
        }
    }

    impl Clock for InMemoryClock {
        fn monotonic_ms(&self) -> u64 {
            self.ticks.get()
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::mem::InMemoryClock;

    fn ok_u64(r: PluginResult) -> u64 {
        match r {
            PluginResult::Ok(resp) => {
                let s = resp.as_slice();
                let mut buf = [0u8; 8];
                buf.copy_from_slice(s);
                u64::from_le_bytes(buf)
            }
            PluginResult::Error(e) => panic!("expected Ok u64, got error code 0x{:02X}: {}", e.code, e.description()),
        }
    }

    fn ok_i64(r: PluginResult) -> i64 {
        match r {
            PluginResult::Ok(resp) => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(resp.as_slice());
                i64::from_le_bytes(buf)
            }
            PluginResult::Error(e) => panic!("expected Ok i64, got error code 0x{:02X}", e.code),
        }
    }

    fn ok_empty(r: PluginResult) {
        match r {
            PluginResult::Ok(resp) => assert!(resp.as_slice().is_empty()),
            PluginResult::Error(e) => panic!("expected Ok empty, got 0x{:02X}", e.code),
        }
    }

    fn err_code(r: PluginResult, expected: u8) {
        match r {
            PluginResult::Ok(_) => panic!("expected error 0x{:02X}, got Ok", expected),
            PluginResult::Error(e) => assert_eq!(e.code, expected),
        }
    }

    #[test]
    fn init_succeeds() {
        let mut p = ClockPlugin::new(InMemoryClock::at(0), 1);
        ok_empty(p.execute(CMD_INIT, &[]));
    }

    #[test]
    fn monotonic_ms_returns_clock_tick() {
        let clock = InMemoryClock::at(12345);
        let mut p = ClockPlugin::new(clock, 1);
        assert_eq!(ok_u64(p.execute(CMD_MONOTONIC_MS, &[])), 12345);
    }

    #[test]
    fn monotonic_ms_is_monotonic() {
        // Use a clock we can advance between calls — the plugin holds it
        // by value, so we test via two separate plugins with two clocks
        // initialised to known values. Real monotonicity is a Clock-impl
        // property — the plugin can't enforce it.
        let mut p1 = ClockPlugin::new(InMemoryClock::at(100), 1);
        let mut p2 = ClockPlugin::new(InMemoryClock::at(200), 1);
        assert!(ok_u64(p1.execute(CMD_MONOTONIC_MS, &[])) < ok_u64(p2.execute(CMD_MONOTONIC_MS, &[])));
    }

    #[test]
    fn now_ms_equals_monotonic_when_offset_is_zero() {
        let clock = InMemoryClock::at(50_000);
        let mut p = ClockPlugin::new(clock, 1);
        let mono = ok_u64(p.execute(CMD_MONOTONIC_MS, &[]));
        let now = ok_u64(p.execute(CMD_NOW_MS, &[]));
        assert_eq!(mono, now);
    }

    #[test]
    fn set_then_get_offset_round_trips() {
        let mut p = ClockPlugin::new(InMemoryClock::at(0), 1);
        let offset = 1_700_000_000_000_i64; // approx Unix-epoch ms in 2023
        ok_empty(p.execute(CMD_SET_OFFSET, &offset.to_le_bytes()));
        assert_eq!(ok_i64(p.execute(CMD_GET_OFFSET, &[])), offset);
    }

    #[test]
    fn now_ms_includes_offset() {
        let clock = InMemoryClock::at(5000);
        let mut p = ClockPlugin::new(clock, 1);
        let offset = 1_000_000_000_i64;
        ok_empty(p.execute(CMD_SET_OFFSET, &offset.to_le_bytes()));
        assert_eq!(ok_u64(p.execute(CMD_NOW_MS, &[])), 5000 + offset as u64);
    }

    #[test]
    fn negative_offset_saturates_at_zero() {
        let clock = InMemoryClock::at(100);
        let mut p = ClockPlugin::new(clock, 1);
        let offset = -1_000_i64;
        ok_empty(p.execute(CMD_SET_OFFSET, &offset.to_le_bytes()));
        // mono(100) + offset(-1000) = -900 → clamped to 0
        assert_eq!(ok_u64(p.execute(CMD_NOW_MS, &[])), 0);
    }

    #[test]
    fn set_offset_wrong_length_errors() {
        let mut p = ClockPlugin::new(InMemoryClock::at(0), 1);
        err_code(p.execute(CMD_SET_OFFSET, &[1, 2, 3]), ERR_BAD_LENGTH);
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = ClockPlugin::new(InMemoryClock::at(0), 1);
        err_code(p.execute(0xAA, &[]), ERR_UNKNOWN_COMMAND);
    }

    #[test]
    fn offset_ms_introspection_matches_get_offset() {
        let mut p = ClockPlugin::new(InMemoryClock::at(0), 1);
        let offset = -42_i64;
        ok_empty(p.execute(CMD_SET_OFFSET, &offset.to_le_bytes()));
        assert_eq!(p.offset_ms(), offset);
        assert_eq!(ok_i64(p.execute(CMD_GET_OFFSET, &[])), offset);
    }
}
