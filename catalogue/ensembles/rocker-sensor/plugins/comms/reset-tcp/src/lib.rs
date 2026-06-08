//! # r2-plugin-comms-reset-tcp
//!
//! R2 AOT plugin: remote-reset TCP listener (port 21044 on the device).
//! A connected client sends a single `CMD_RESET` byte (`0x52`) and the
//! device reboots — SPEC-R2-WORKSHOP-SENSOR-REMOTE-RESET, the
//! dashboard's `r2.dash.reset` flow.
//!
//! **Reference:** `r2-workshop` firmware `r2_esp::reset_tcp`
//! (`start_listener` on TCP 21044). The TCP accept loop + `esp_restart()`
//! are platform IO; this crate is the **protocol core** (recognise the
//! reset byte) plus a [`Resetter`] hook the firmware-render step backs
//! with `esp_restart()`. That keeps the crate `no_std` + host-testable.
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name   | Effect |
//! |------|--------|--------|
//! | 0x01 | init   | arm the listener (idempotent) |
//! | 0x02 | listen | arm the listener (idempotent alias) |
//!
//! Inbound client bytes do **not** arrive over the R2 command bus — the
//! firmware's accept loop reads them and calls [`ResetTcp::on_client_byte`].

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Platform hook: reboot the device. The firmware-render step backs this
/// with `esp_restart()`; tests supply a spy.
pub trait Resetter {
    /// Reboot now. (On real hardware this does not return.)
    fn reset(&mut self);
}

/// The single byte a client sends to trigger a reset (datasheet of the
/// workshop protocol: ASCII 'R').
pub const CMD_RESET: u8 = 0x52;

/// True iff `b` is the reset command byte.
pub fn is_reset_byte(b: u8) -> bool {
    b == CMD_RESET
}

/// Command opcode: arm the listener.
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: arm the listener (idempotent alias).
pub const CMD_LISTEN: PluginCommand = 0x02;

/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Remote-reset listener plugin, generic over a [`Resetter`].
pub struct ResetTcp<R: Resetter> {
    resetter: R,
    id: PluginId,
    listening: bool,
}

impl<R: Resetter> ResetTcp<R> {
    /// Construct, dis-armed, bound to `id`.
    pub const fn new(resetter: R, id: PluginId) -> Self {
        Self { resetter, id, listening: false }
    }

    /// Firmware accept-loop hook: feed one byte read from a client.
    /// Returns `true` (and triggers the reset) iff armed and `b` is
    /// [`CMD_RESET`]. Not part of the R2 command bus.
    pub fn on_client_byte(&mut self, b: u8) -> bool {
        if self.listening && is_reset_byte(b) {
            self.resetter.reset();
            true
        } else {
            false
        }
    }

    /// Whether the listener is armed.
    pub fn is_listening(&self) -> bool {
        self.listening
    }
}

impl<R: Resetter> Plugin for ResetTcp<R> {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT | CMD_LISTEN => {
                self.listening = true;
                PluginResult::Ok(PluginResponse::empty())
            }
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "reset-tcp: unknown command byte")),
        }
    }
    fn name(&self) -> &str {
        "comms/reset-tcp"
    }
    fn id(&self) -> PluginId {
        self.id
    }
    fn init(&mut self) -> PluginResult {
        self.listening = true;
        PluginResult::Ok(PluginResponse::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SpyReset {
        count: usize,
    }
    impl Resetter for SpyReset {
        fn reset(&mut self) {
            self.count += 1;
        }
    }

    fn armed() -> ResetTcp<SpyReset> {
        let mut p = ResetTcp::new(SpyReset { count: 0 }, 7);
        assert!(matches!(p.execute(CMD_INIT, &[]), PluginResult::Ok(_)));
        p
    }

    #[test]
    fn reset_byte_recognised() {
        assert!(is_reset_byte(0x52));
        assert!(!is_reset_byte(0x00));
        assert!(!is_reset_byte(b'r')); // lowercase is not the command
    }

    #[test]
    fn armed_reset_byte_triggers_reset() {
        let mut p = armed();
        assert!(p.on_client_byte(CMD_RESET));
        assert_eq!(p.resetter.count, 1);
    }

    #[test]
    fn other_bytes_ignored() {
        let mut p = armed();
        assert!(!p.on_client_byte(0x00));
        assert!(!p.on_client_byte(b'X'));
        assert_eq!(p.resetter.count, 0);
    }

    #[test]
    fn not_armed_does_not_reset() {
        let mut p = ResetTcp::new(SpyReset { count: 0 }, 7);
        assert!(!p.is_listening());
        assert!(!p.on_client_byte(CMD_RESET));
        assert_eq!(p.resetter.count, 0);
    }

    #[test]
    fn listen_is_idempotent_alias() {
        let mut p = ResetTcp::new(SpyReset { count: 0 }, 7);
        assert!(matches!(p.execute(CMD_LISTEN, &[]), PluginResult::Ok(_)));
        assert!(matches!(p.execute(CMD_LISTEN, &[]), PluginResult::Ok(_)));
        assert!(p.is_listening());
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = ResetTcp::new(SpyReset { count: 0 }, 7);
        let PluginResult::Error(e) = p.execute(0xAA, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}
