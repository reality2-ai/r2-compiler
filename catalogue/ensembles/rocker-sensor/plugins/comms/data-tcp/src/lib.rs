//! # r2-plugin-comms-data-tcp
//!
//! R2 AOT plugin: capture-file TCP server (port 21047). Implements
//! `LIST` / `GET <name>` / `DEL <name>` / `DEL_ALL` over the captures
//! directory on the SD card — the dashboard downloads named capture
//! files and frees SD space remotely (SPEC-R2-WORKSHOP-CAPTURE §6).
//!
//! **Reference:** `r2-workshop` firmware `r2_esp::data_tcp`
//! (`start_listener` on TCP 21047). The TCP accept loop is platform IO;
//! this crate is the **request protocol core** ([`parse_request`] +
//! [`DataTcp::handle_request`]) over a [`CaptureStore`] hook the
//! firmware-render step backs with the SD captures dir (storage/sd-card).
//! `no_std` + `alloc`, host-testable.
//!
//! ## Wire protocol (one text request per connection)
//!
//! | Request    | Response |
//! |------------|----------|
//! | `LIST`     | file names, one per line |
//! | `GET <n>`  | the file's bytes, or `ERR not_found` |
//! | `DEL <n>`  | `OK` or `ERR not_found` |
//! | `DEL_ALL`  | `OK <count>` |
//! | (other)    | `ERR bad_request` |
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name | Effect |
//! |------|------|--------|
//! | 0x01 | init | start the server (idempotent) |
//!
//! Requests arrive on the TCP socket, not the R2 command bus — the
//! firmware's accept loop calls [`DataTcp::handle_request`].

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// Platform hook: the captures directory on the SD card. The
/// firmware-render step backs this with storage/sd-card; tests use an
/// in-memory map.
pub trait CaptureStore {
    /// All capture file names.
    fn list(&mut self) -> Vec<String>;
    /// Full contents of `name`, or `None` if absent. (Whole-file for the
    /// protocol core; the firmware streams it chunk-wise on the socket.)
    fn get(&mut self, name: &str) -> Option<Vec<u8>>;
    /// Delete `name`; `true` if it existed.
    fn delete(&mut self, name: &str) -> bool;
    /// Delete every capture; returns the count removed.
    fn delete_all(&mut self) -> usize;
}

/// A parsed client request.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DataCmd {
    /// `LIST`
    List,
    /// `GET <name>`
    Get(String),
    /// `DEL <name>`
    Del(String),
    /// `DEL_ALL`
    DelAll,
    /// Anything else.
    Unknown,
}

/// Parse one text request line into a [`DataCmd`] (verb is
/// case-insensitive; name is whitespace-delimited).
pub fn parse_request(line: &str) -> DataCmd {
    let mut it = line.split_whitespace();
    let Some(verb) = it.next() else { return DataCmd::Unknown };
    let upper = {
        // ASCII-uppercase the verb without alloc-churn beyond one String.
        let mut s = String::with_capacity(verb.len());
        for c in verb.chars() {
            s.push(c.to_ascii_uppercase());
        }
        s
    };
    match upper.as_str() {
        "LIST" => DataCmd::List,
        "DEL_ALL" => DataCmd::DelAll,
        "GET" => match it.next() {
            Some(n) => DataCmd::Get(String::from(n)),
            None => DataCmd::Unknown,
        },
        "DEL" => match it.next() {
            Some(n) => DataCmd::Del(String::from(n)),
            None => DataCmd::Unknown,
        },
        _ => DataCmd::Unknown,
    }
}

/// Command opcode: start the server.
pub const CMD_INIT: PluginCommand = 0x01;

/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Capture-file TCP server plugin, generic over a [`CaptureStore`].
pub struct DataTcp<C: CaptureStore> {
    store: C,
    id: PluginId,
    started: bool,
}

impl<C: CaptureStore> DataTcp<C> {
    /// Construct (not started) bound to `id`.
    pub const fn new(store: C, id: PluginId) -> Self {
        Self { store, id, started: false }
    }

    /// Firmware accept-loop hook: handle one client request line, return
    /// the response bytes to write back. Not part of the R2 command bus.
    pub fn handle_request(&mut self, line: &str) -> Vec<u8> {
        match parse_request(line) {
            DataCmd::List => self.store.list().join("\n").into_bytes(),
            DataCmd::Get(name) => match self.store.get(&name) {
                Some(bytes) => bytes,
                None => b"ERR not_found".to_vec(),
            },
            DataCmd::Del(name) => {
                if self.store.delete(&name) {
                    b"OK".to_vec()
                } else {
                    b"ERR not_found".to_vec()
                }
            }
            DataCmd::DelAll => {
                let n = self.store.delete_all();
                format!("OK {n}").into_bytes()
            }
            DataCmd::Unknown => b"ERR bad_request".to_vec(),
        }
    }

    /// Whether the server has been started.
    pub fn is_started(&self) -> bool {
        self.started
    }
}

impl<C: CaptureStore> Plugin for DataTcp<C> {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => {
                self.started = true;
                PluginResult::Ok(PluginResponse::empty())
            }
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "data-tcp: unknown command byte")),
        }
    }
    fn name(&self) -> &str {
        "comms/data-tcp"
    }
    fn id(&self) -> PluginId {
        self.id
    }
    fn init(&mut self) -> PluginResult {
        self.started = true;
        PluginResult::Ok(PluginResponse::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

    struct MemStore {
        files: BTreeMap<String, Vec<u8>>,
    }
    impl CaptureStore for MemStore {
        fn list(&mut self) -> Vec<String> {
            self.files.keys().cloned().collect()
        }
        fn get(&mut self, name: &str) -> Option<Vec<u8>> {
            self.files.get(name).cloned()
        }
        fn delete(&mut self, name: &str) -> bool {
            self.files.remove(name).is_some()
        }
        fn delete_all(&mut self) -> usize {
            let n = self.files.len();
            self.files.clear();
            n
        }
    }

    fn store() -> MemStore {
        let mut files = BTreeMap::new();
        files.insert("a.csv".to_string(), b"alpha".to_vec());
        files.insert("b.csv".to_string(), b"bravo".to_vec());
        MemStore { files }
    }

    fn started() -> DataTcp<MemStore> {
        let mut p = DataTcp::new(store(), 7);
        assert!(matches!(p.execute(CMD_INIT, &[]), PluginResult::Ok(_)));
        p
    }

    #[test]
    fn parse_all_verbs() {
        assert_eq!(parse_request("LIST"), DataCmd::List);
        assert_eq!(parse_request("list"), DataCmd::List); // case-insensitive
        assert_eq!(parse_request("DEL_ALL"), DataCmd::DelAll);
        assert_eq!(parse_request("GET a.csv"), DataCmd::Get("a.csv".into()));
        assert_eq!(parse_request("DEL b.csv"), DataCmd::Del("b.csv".into()));
        assert_eq!(parse_request("GET"), DataCmd::Unknown); // missing arg
        assert_eq!(parse_request("FROBNICATE"), DataCmd::Unknown);
        assert_eq!(parse_request(""), DataCmd::Unknown);
    }

    #[test]
    fn list_returns_names() {
        let mut p = started();
        let resp = p.handle_request("LIST");
        let s = core::str::from_utf8(&resp).unwrap();
        let mut names: Vec<&str> = s.split('\n').collect();
        names.sort();
        assert_eq!(names, vec!["a.csv", "b.csv"]);
    }

    #[test]
    fn get_returns_bytes_or_not_found() {
        let mut p = started();
        assert_eq!(p.handle_request("GET a.csv"), b"alpha".to_vec());
        assert_eq!(p.handle_request("GET missing.csv"), b"ERR not_found".to_vec());
    }

    #[test]
    fn del_and_del_all() {
        let mut p = started();
        assert_eq!(p.handle_request("DEL a.csv"), b"OK".to_vec());
        assert_eq!(p.handle_request("DEL a.csv"), b"ERR not_found".to_vec());
        // one file left → DEL_ALL removes 1
        assert_eq!(p.handle_request("DEL_ALL"), b"OK 1".to_vec());
        assert!(p.store.list().is_empty());
    }

    #[test]
    fn bad_request() {
        let mut p = started();
        assert_eq!(p.handle_request("HACK the planet"), b"ERR bad_request".to_vec());
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = DataTcp::new(store(), 7);
        let PluginResult::Error(e) = p.execute(0xAA, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}
