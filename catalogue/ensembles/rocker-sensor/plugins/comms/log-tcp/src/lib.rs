//! # r2-plugin-comms-log-tcp
//!
//! R2 AOT plugin: read-only fan-out of the device's `log::info!/warn!/error!`
//! stream to TCP clients (port 21046). Multiple dashboard clients can
//! `nc <ip> 21046` to tail the live log
//! (SPEC-R2-WORKSHOP-SENSOR-LIVE-LOGS). Anything a client sends is ignored.
//!
//! **Reference:** `r2-workshop` firmware `r2_esp::log_tcp`
//! (`install_logger` + `start_listener` on TCP 21046). The TCP accept
//! loop + per-client sockets are platform IO; this crate is the
//! **fan-out core** plus a [`LogSink`] hook the firmware-render step backs
//! with its connected-client set. `no_std`, host-testable.
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name | Effect |
//! |------|------|--------|
//! | 0x01 | init | install the fan-out (idempotent) |
//!
//! Log lines do not arrive over the R2 command bus — the firmware's
//! logger calls [`LogTcp::on_log_line`] for each line.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Platform hook: broadcast one log line to every connected client. The
/// firmware-render step backs this with its TCP client set; tests spy on
/// it. `Err(())` means the whole sink failed (e.g. listener down).
pub trait LogSink {
    /// Broadcast `line` to all currently-connected clients.
    fn broadcast(&mut self, line: &str) -> Result<(), ()>;
}

/// Command opcode: install the fan-out.
pub const CMD_INIT: PluginCommand = 0x01;

/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Live-log fan-out plugin, generic over a [`LogSink`].
pub struct LogTcp<S: LogSink> {
    sink: S,
    id: PluginId,
    installed: bool,
}

impl<S: LogSink> LogTcp<S> {
    /// Construct (un-installed) bound to `id`.
    pub const fn new(sink: S, id: PluginId) -> Self {
        Self { sink, id, installed: false }
    }

    /// Firmware logger hook: fan one log line out to all clients. No-op
    /// (returns `false`) until `init`. Returns `true` iff the broadcast
    /// succeeded. Not part of the R2 command bus.
    pub fn on_log_line(&mut self, line: &str) -> bool {
        self.installed && self.sink.broadcast(line).is_ok()
    }

    /// Whether the fan-out is installed.
    pub fn is_installed(&self) -> bool {
        self.installed
    }
}

impl<S: LogSink> Plugin for LogTcp<S> {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => {
                self.installed = true;
                PluginResult::Ok(PluginResponse::empty())
            }
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "log-tcp: unknown command byte")),
        }
    }
    fn name(&self) -> &str {
        "comms/log-tcp"
    }
    fn id(&self) -> PluginId {
        self.id
    }
    fn init(&mut self) -> PluginResult {
        self.installed = true;
        PluginResult::Ok(PluginResponse::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SpySink {
        lines: std::vec::Vec<std::string::String>,
        fail: bool,
    }
    impl LogSink for SpySink {
        fn broadcast(&mut self, line: &str) -> Result<(), ()> {
            if self.fail {
                return Err(());
            }
            self.lines.push(line.into());
            Ok(())
        }
    }

    fn installed() -> LogTcp<SpySink> {
        let mut p = LogTcp::new(SpySink { lines: vec![], fail: false }, 7);
        assert!(matches!(p.execute(CMD_INIT, &[]), PluginResult::Ok(_)));
        p
    }

    #[test]
    fn installed_lines_fan_out() {
        let mut p = installed();
        assert!(p.on_log_line("hello"));
        assert!(p.on_log_line("world"));
        assert_eq!(p.sink.lines, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn not_installed_drops_lines() {
        let mut p = LogTcp::new(SpySink { lines: vec![], fail: false }, 7);
        assert!(!p.is_installed());
        assert!(!p.on_log_line("x"));
        assert!(p.sink.lines.is_empty());
    }

    #[test]
    fn sink_failure_reported() {
        let mut p = LogTcp::new(SpySink { lines: vec![], fail: true }, 7);
        p.execute(CMD_INIT, &[]);
        assert!(!p.on_log_line("x"));
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = LogTcp::new(SpySink { lines: vec![], fail: false }, 7);
        let PluginResult::Error(e) = p.execute(0xAA, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}
