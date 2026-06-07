//! # r2-plugin-storage-sd-card
//!
//! R2 AOT plugin: durable sample storage on a FATFS-mounted microSD card.
//! Two responsibilities, per r2-workshop SPEC-R2-WORKSHOP-SENSOR §6 (the
//! "CAPTURE" subsystem):
//!
//! 1. **The acceleration ring** — fixed-width 62-byte CSV records appended
//!    to `logNNNN.csv` segments, rotated at a size threshold, with the
//!    oldest segment freed once a cumulative ack arrives. Survives reboots:
//!    on `init` it scans the card, finds the highest segment, and recovers
//!    the tail sequence number from the last record.
//! 2. **Capture-file management** — open / write / close / list / get /
//!    delete arbitrary files (named recordings), plus an explicit `sync`.
//!
//! **Reference implementation:** `r2-workshop/firmware/esp32-s3/devkitc/src/`
//! `ring.rs` (the ring logic + record format, ported here verbatim in
//! semantics) and `sd.rs` (the esp-idf FATFS mount — pure platform IO,
//! which *becomes* the firmware's [`SdFs`] impl rather than part of this
//! crate).
//!
//! ## Filesystem abstraction
//!
//! All IO is behind the minimal [`SdFs`] trait. The firmware-render step
//! wraps esp-idf's FATFS-over-SD-SPI (`std::fs` against the `/sdcard`
//! mount); tests use an in-memory map. This is the only platform hook, and
//! it's why the crate is host-testable.
//!
//! ## `alloc`
//!
//! Unlike the other ensemble plugins this one is `no_std` **+ `alloc`**:
//! file lists are `Vec<String>` and file contents are variable-length.
//! On-target esp-idf provides the global allocator.
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name        | Input | Output |
//! |------|-------------|-------|--------|
//! | 0x01 | init        | `[]` | `[tail_seq: u32 LE]` |
//! | 0x02 | ring_push   | `[seq u32 LE | ts_ms u32 LE | x i32 LE | y i32 LE | z i32 LE]` (20 bytes) | `[]` |
//! | 0x03 | ring_pop    | `[through_seq: u32 LE]` | `[freed_segments: u32 LE]` |
//! | 0x04 | file_open   | `[name: utf-8]` | `[]` |
//! | 0x05 | file_write  | `[data: bytes]` (appended to the open file) | `[written: u32 LE]` |
//! | 0x06 | file_close  | `[]` | `[]` |
//! | 0x07 | file_list   | `[]` | names joined by `\n` (utf-8) |
//! | 0x08 | file_get    | `[off u32 LE | len u16 LE | name: utf-8]` | up to `len` bytes |
//! | 0x09 | file_delete | `[name: utf-8]` | `[]` |
//! | 0x0A | sync        | `[]` | `[]` |
//!
//! `ring_push` takes the 20-byte binary sample; this crate formats it into
//! the 62-byte CSV row for storage (`ts_ms` is widened to `i64` for the
//! CSV column, matching the reference).

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

// ── SdFs backend trait ────────────────────────────────────────────────

/// A filesystem error from the backend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FsError {
    /// The named file does not exist.
    NotFound,
    /// Any other backend IO failure.
    Io,
}

/// Minimal flat-namespace filesystem the plugin needs. The firmware-render
/// step supplies an esp-idf FATFS impl (over the `/sdcard` mount); tests
/// supply an in-memory one. Paths are bare file names at the mount root
/// (the ring deviates from the spec's `/r2/` subdir for the same
/// ESP-IDF-FATFS reason the reference module documents).
pub trait SdFs {
    /// List all file names at the root.
    fn list(&mut self) -> Result<Vec<String>, FsError>;
    /// Byte length of `name`.
    fn size(&mut self, name: &str) -> Result<u64, FsError>;
    /// Read into `buf` starting at `off`; returns bytes read (may be short at EOF).
    fn read_at(&mut self, name: &str, off: u64, buf: &mut [u8]) -> Result<usize, FsError>;
    /// Write `data` at `off`, creating/extending `name` as needed.
    fn write_at(&mut self, name: &str, off: u64, data: &[u8]) -> Result<(), FsError>;
    /// Append `data` to `name` (creating it if absent).
    fn append(&mut self, name: &str, data: &[u8]) -> Result<(), FsError>;
    /// Remove `name`.
    fn remove(&mut self, name: &str) -> Result<(), FsError>;
    /// Flush buffered writes durably to the card.
    fn sync(&mut self) -> Result<(), FsError>;
}

// ── Ring record format (SPEC-R2-WORKSHOP-SENSOR §6.2 v0.2 CSV) ─────────

/// Bytes per ring record. Must agree with [`encode_record`]'s layout —
/// boot recovery's seek arithmetic depends on it.
pub const RECORD_BYTES: u64 = 62;
const W_SEQ: usize = 10;
const W_TS_MS: usize = 14;
const W_AXIS: usize = 11;

/// Default segment size (8 MiB ≈ 35 min at 100 Hz with 62-byte records).
pub const DEFAULT_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
/// Default ring depth — the oldest segment is freed past this count.
pub const DEFAULT_RING_SEGMENTS: usize = 12;

/// Format one fixed-width CSV record. Returns `None` if any value
/// overflows its column (which would break the fixed-width invariant —
/// surfaced as an error rather than silently corrupting the seek layout).
pub fn encode_record(seq: u32, ts_ms: i64, x: i32, y: i32, z: i32) -> Option<[u8; RECORD_BYTES as usize]> {
    let s = format!(
        "{seq:>W_SEQ$},{ts_ms:>W_TS_MS$},{x:>W_AXIS$},{y:>W_AXIS$},{z:>W_AXIS$}\n",
        W_SEQ = W_SEQ, W_TS_MS = W_TS_MS, W_AXIS = W_AXIS,
    );
    let bytes = s.as_bytes();
    if bytes.len() != RECORD_BYTES as usize {
        return None;
    }
    let mut out = [0u8; RECORD_BYTES as usize];
    out.copy_from_slice(bytes);
    Some(out)
}

/// Parse the right-aligned `seq` column (first [`W_SEQ`] bytes of a record).
fn parse_seq_field(buf: &[u8]) -> Option<u32> {
    core::str::from_utf8(buf).ok()?.trim().parse::<u32>().ok()
}

/// Segment file name for `num` (`logNNNN.csv`, strict 8.3 for FATFS).
fn segment_name(num: u32) -> String {
    format!("log{num:04}.csv")
}

/// Parse a segment number from a file name, or `None` if it isn't one.
fn parse_segment_name(name: &str) -> Option<u32> {
    name.strip_prefix("log")?.strip_suffix(".csv")?.parse().ok()
}

// ── Plugin ────────────────────────────────────────────────────────────

/// Command opcode: mount + boot-recover the ring.
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: append a sample to the ring.
pub const CMD_RING_PUSH: PluginCommand = 0x02;
/// Command opcode: free ring segments fully acked through a seq.
pub const CMD_RING_POP: PluginCommand = 0x03;
/// Command opcode: open (create) a capture file for writing.
pub const CMD_FILE_OPEN: PluginCommand = 0x04;
/// Command opcode: append data to the open capture file.
pub const CMD_FILE_WRITE: PluginCommand = 0x05;
/// Command opcode: close (sync) the open capture file.
pub const CMD_FILE_CLOSE: PluginCommand = 0x06;
/// Command opcode: list file names.
pub const CMD_FILE_LIST: PluginCommand = 0x07;
/// Command opcode: read a chunk of a file.
pub const CMD_FILE_GET: PluginCommand = 0x08;
/// Command opcode: delete a file.
pub const CMD_FILE_DELETE: PluginCommand = 0x09;
/// Command opcode: fsync the filesystem.
pub const CMD_SYNC: PluginCommand = 0x0A;

/// Error code: input byte length did not match the command's layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error code: a filesystem backend operation failed.
pub const ERR_FS: u8 = 0x02;
/// Error code: a ring/file command was issued before `init`.
pub const ERR_NOT_INIT: u8 = 0x03;
/// Error code: `file_write`/`file_close` with no file open.
pub const ERR_NO_OPEN_FILE: u8 = 0x04;
/// Error code: a record value overflowed its fixed-width column.
pub const ERR_RECORD_FMT: u8 = 0x05;
/// Error code: a named file was not found.
pub const ERR_NOT_FOUND: u8 = 0x06;
/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// microSD ring + capture-file plugin, generic over an [`SdFs`] backend.
pub struct SdCard<F: SdFs> {
    fs: F,
    id: PluginId,
    initialised: bool,
    // Ring state.
    segment_bytes: u64,
    ring_segments: usize,
    current_num: u32,
    current_bytes: u64,
    tail_seq: u32,
    // Capture-file state (single open write target).
    open_file: Option<String>,
    open_offset: u64,
}

impl<F: SdFs> SdCard<F> {
    /// Construct with the default 8 MiB segment size and 12-segment depth.
    pub fn new(fs: F, id: PluginId) -> Self {
        Self::with_limits(fs, id, DEFAULT_SEGMENT_BYTES, DEFAULT_RING_SEGMENTS)
    }

    /// Construct with explicit ring limits (tests use tiny values to
    /// exercise rotation + eviction without writing megabytes).
    pub fn with_limits(fs: F, id: PluginId, segment_bytes: u64, ring_segments: usize) -> Self {
        Self {
            fs,
            id,
            initialised: false,
            segment_bytes: segment_bytes.max(RECORD_BYTES),
            ring_segments: ring_segments.max(1),
            current_num: 1,
            current_bytes: 0,
            tail_seq: 0,
            open_file: None,
            open_offset: 0,
        }
    }

    fn sorted_segments(&mut self) -> Result<Vec<u32>, FsError> {
        let mut segs: Vec<u32> = self
            .fs
            .list()?
            .iter()
            .filter_map(|n| parse_segment_name(n))
            .collect();
        segs.sort_unstable();
        Ok(segs)
    }

    fn read_last_seq(&mut self, num: u32, n_records: u64) -> Result<u32, FsError> {
        let mut buf = [0u8; W_SEQ];
        let off = (n_records - 1) * RECORD_BYTES;
        let n = self.fs.read_at(&segment_name(num), off, &mut buf)?;
        if n < W_SEQ {
            return Err(FsError::Io);
        }
        parse_seq_field(&buf).ok_or(FsError::Io)
    }

    // ── Command handlers ──────────────────────────────────────────────

    /// Boot recovery (§6.5): find the highest segment + recover tail_seq.
    fn op_init(&mut self) -> PluginResult {
        let segs = match self.sorted_segments() {
            Ok(s) => s,
            Err(_) => return fs_err("init: list segments"),
        };
        match segs.last() {
            Some(&highest) => {
                let size = match self.fs.size(&segment_name(highest)) {
                    Ok(s) => s,
                    Err(_) => return fs_err("init: size"),
                };
                let n_records = size / RECORD_BYTES;
                self.current_num = highest;
                self.current_bytes = size;
                self.tail_seq = if n_records == 0 {
                    0
                } else {
                    match self.read_last_seq(highest, n_records) {
                        Ok(s) => s,
                        Err(_) => return fs_err("init: recover tail_seq"),
                    }
                };
            }
            None => {
                self.current_num = 1;
                self.current_bytes = 0;
                self.tail_seq = 0;
            }
        }
        self.initialised = true;
        PluginResult::Ok(PluginResponse::with_data(&self.tail_seq.to_le_bytes()))
    }

    fn op_ring_push(&mut self, data: &[u8]) -> PluginResult {
        if !self.initialised {
            return PluginResult::Error(PluginError::new(ERR_NOT_INIT, "sd-card: ring_push before init"));
        }
        if data.len() != 20 {
            return PluginResult::Error(PluginError::new(
                ERR_BAD_LENGTH,
                "sd-card: ring_push expects 20 bytes [seq,ts_ms,x,y,z]",
            ));
        }
        let seq = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let ts_ms = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as i64;
        let x = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let y = i32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let z = i32::from_le_bytes([data[16], data[17], data[18], data[19]]);

        let record = match encode_record(seq, ts_ms, x, y, z) {
            Some(r) => r,
            None => return PluginResult::Error(PluginError::new(
                ERR_RECORD_FMT,
                "sd-card: record value overflowed its fixed-width column",
            )),
        };

        // Rotate if this record would exceed the segment threshold.
        if self.current_bytes.saturating_add(RECORD_BYTES) > self.segment_bytes {
            if let Err(()) = self.rotate() {
                return fs_err("ring_push: rotate");
            }
        }
        if self.fs.append(&segment_name(self.current_num), &record).is_err() {
            return fs_err("ring_push: append");
        }
        self.current_bytes += RECORD_BYTES;
        self.tail_seq = seq;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn rotate(&mut self) -> Result<(), ()> {
        let _ = self.fs.sync();
        self.current_num = self.current_num.checked_add(1).unwrap_or(1);
        self.current_bytes = 0;
        // Materialise the new segment as an (empty) file now — like the
        // reference's `File::create` in rotate — so the depth enforcement
        // below counts it and the `oldest == current_num` guard protects
        // it. Without this the eviction can't see the new current and the
        // ring drifts to depth+1 segments.
        self.fs.write_at(&segment_name(self.current_num), 0, &[]).map_err(|_| ())?;
        // Enforce ring depth: drop oldest segments past the cap (never the
        // current write target).
        let mut segs = self.sorted_segments().map_err(|_| ())?;
        while segs.len() > self.ring_segments {
            let oldest = segs.remove(0);
            if oldest == self.current_num {
                break;
            }
            if self.fs.remove(&segment_name(oldest)).is_err() {
                break;
            }
        }
        Ok(())
    }

    /// Free segments whose last record is `seq ≤ through_seq` (§7.4).
    /// Never frees the current write target. Returns the count freed.
    fn op_ring_pop(&mut self, data: &[u8]) -> PluginResult {
        if !self.initialised {
            return PluginResult::Error(PluginError::new(ERR_NOT_INIT, "sd-card: ring_pop before init"));
        }
        if data.len() != 4 {
            return PluginResult::Error(PluginError::new(
                ERR_BAD_LENGTH,
                "sd-card: ring_pop expects 4 bytes [through_seq u32 LE]",
            ));
        }
        let through_seq = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let segs = match self.sorted_segments() {
            Ok(s) => s,
            Err(_) => return fs_err("ring_pop: list"),
        };
        let mut freed: u32 = 0;
        for num in segs {
            if num == self.current_num {
                break;
            }
            let size = match self.fs.size(&segment_name(num)) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let n_records = size / RECORD_BYTES;
            if n_records == 0 {
                if self.fs.remove(&segment_name(num)).is_ok() {
                    freed += 1;
                }
                continue;
            }
            let last_seq = match self.read_last_seq(num, n_records) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if last_seq > through_seq {
                break; // first un-acked segment — everything above is too
            }
            if self.fs.remove(&segment_name(num)).is_ok() {
                freed += 1;
            }
        }
        PluginResult::Ok(PluginResponse::with_data(&freed.to_le_bytes()))
    }

    fn op_file_open(&mut self, data: &[u8]) -> PluginResult {
        if !self.initialised {
            return PluginResult::Error(PluginError::new(ERR_NOT_INIT, "sd-card: file_open before init"));
        }
        let name = match parse_name(data) {
            Some(n) => n,
            None => return PluginResult::Error(PluginError::new(ERR_BAD_LENGTH, "sd-card: file_open empty/invalid name")),
        };
        // Open-for-append semantics: position at current end (0 if new).
        let off = self.fs.size(&name).unwrap_or(0);
        self.open_file = Some(name);
        self.open_offset = off;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn op_file_write(&mut self, data: &[u8]) -> PluginResult {
        let (name, off) = match &self.open_file {
            Some(n) => (n.clone(), self.open_offset),
            None => return PluginResult::Error(PluginError::new(ERR_NO_OPEN_FILE, "sd-card: file_write with no open file")),
        };
        if self.fs.write_at(&name, off, data).is_err() {
            return fs_err("file_write");
        }
        self.open_offset += data.len() as u64;
        PluginResult::Ok(PluginResponse::with_data(&(data.len() as u32).to_le_bytes()))
    }

    fn op_file_close(&mut self) -> PluginResult {
        if self.open_file.is_none() {
            return PluginResult::Error(PluginError::new(ERR_NO_OPEN_FILE, "sd-card: file_close with no open file"));
        }
        let _ = self.fs.sync();
        self.open_file = None;
        self.open_offset = 0;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn op_file_list(&mut self) -> PluginResult {
        let names = match self.fs.list() {
            Ok(n) => n,
            Err(_) => return fs_err("file_list"),
        };
        PluginResult::Ok(PluginResponse::with_data(names.join("\n").as_bytes()))
    }

    fn op_file_get(&mut self, data: &[u8]) -> PluginResult {
        if data.len() < 6 {
            return PluginResult::Error(PluginError::new(
                ERR_BAD_LENGTH,
                "sd-card: file_get expects [off u32 LE | len u16 LE | name]",
            ));
        }
        let off = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as u64;
        let len = u16::from_le_bytes([data[4], data[5]]) as usize;
        let name = match core::str::from_utf8(&data[6..]) {
            Ok(s) if !s.is_empty() => s,
            _ => return PluginResult::Error(PluginError::new(ERR_BAD_LENGTH, "sd-card: file_get empty/invalid name")),
        };
        let mut buf = vec![0u8; len];
        match self.fs.read_at(name, off, &mut buf) {
            Ok(n) => {
                buf.truncate(n);
                PluginResult::Ok(PluginResponse::with_data(&buf))
            }
            Err(FsError::NotFound) => PluginResult::Error(PluginError::new(ERR_NOT_FOUND, "sd-card: file_get no such file")),
            Err(FsError::Io) => fs_err("file_get"),
        }
    }

    fn op_file_delete(&mut self, data: &[u8]) -> PluginResult {
        let name = match parse_name(data) {
            Some(n) => n,
            None => return PluginResult::Error(PluginError::new(ERR_BAD_LENGTH, "sd-card: file_delete empty/invalid name")),
        };
        match self.fs.remove(&name) {
            Ok(()) => {
                if self.open_file.as_deref() == Some(name.as_str()) {
                    self.open_file = None;
                    self.open_offset = 0;
                }
                PluginResult::Ok(PluginResponse::empty())
            }
            Err(FsError::NotFound) => PluginResult::Error(PluginError::new(ERR_NOT_FOUND, "sd-card: file_delete no such file")),
            Err(FsError::Io) => fs_err("file_delete"),
        }
    }

    fn op_sync(&mut self) -> PluginResult {
        match self.fs.sync() {
            Ok(()) => PluginResult::Ok(PluginResponse::empty()),
            Err(_) => fs_err("sync"),
        }
    }
}

impl<F: SdFs> Plugin for SdCard<F> {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT => self.op_init(),
            CMD_RING_PUSH => self.op_ring_push(data),
            CMD_RING_POP => self.op_ring_pop(data),
            CMD_FILE_OPEN => self.op_file_open(data),
            CMD_FILE_WRITE => self.op_file_write(data),
            CMD_FILE_CLOSE => self.op_file_close(),
            CMD_FILE_LIST => self.op_file_list(),
            CMD_FILE_GET => self.op_file_get(data),
            CMD_FILE_DELETE => self.op_file_delete(data),
            CMD_SYNC => self.op_sync(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "sd-card: unknown command byte")),
        }
    }

    fn name(&self) -> &str {
        "storage/sd-card"
    }

    fn id(&self) -> PluginId {
        self.id
    }

    fn init(&mut self) -> PluginResult {
        self.op_init()
    }
}

fn fs_err(ctx: &'static str) -> PluginResult {
    let _ = ctx;
    PluginResult::Error(PluginError::new(ERR_FS, "sd-card: filesystem backend error"))
}

/// Validate a non-empty UTF-8 file name from raw command bytes.
fn parse_name(data: &[u8]) -> Option<String> {
    let s = core::str::from_utf8(data).ok()?;
    if s.is_empty() {
        None
    } else {
        Some(String::from(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;

    /// In-memory SdFs for tests.
    struct MemFs {
        files: BTreeMap<String, Vec<u8>>,
        syncs: usize,
    }
    impl MemFs {
        fn new() -> Self {
            Self { files: BTreeMap::new(), syncs: 0 }
        }
    }
    impl SdFs for MemFs {
        fn list(&mut self) -> Result<Vec<String>, FsError> {
            Ok(self.files.keys().cloned().collect())
        }
        fn size(&mut self, name: &str) -> Result<u64, FsError> {
            self.files.get(name).map(|v| v.len() as u64).ok_or(FsError::NotFound)
        }
        fn read_at(&mut self, name: &str, off: u64, buf: &mut [u8]) -> Result<usize, FsError> {
            let v = self.files.get(name).ok_or(FsError::NotFound)?;
            let off = off as usize;
            if off >= v.len() {
                return Ok(0);
            }
            let n = buf.len().min(v.len() - off);
            buf[..n].copy_from_slice(&v[off..off + n]);
            Ok(n)
        }
        fn write_at(&mut self, name: &str, off: u64, data: &[u8]) -> Result<(), FsError> {
            let v = self.files.entry(name.to_string()).or_default();
            let off = off as usize;
            if v.len() < off + data.len() {
                v.resize(off + data.len(), 0);
            }
            v[off..off + data.len()].copy_from_slice(data);
            Ok(())
        }
        fn append(&mut self, name: &str, data: &[u8]) -> Result<(), FsError> {
            self.files.entry(name.to_string()).or_default().extend_from_slice(data);
            Ok(())
        }
        fn remove(&mut self, name: &str) -> Result<(), FsError> {
            self.files.remove(name).map(|_| ()).ok_or(FsError::NotFound)
        }
        fn sync(&mut self) -> Result<(), FsError> {
            self.syncs += 1;
            Ok(())
        }
    }

    fn push(p: &mut SdCard<MemFs>, seq: u32, ts: u32, x: i32, y: i32, z: i32) -> PluginResult {
        let mut d = Vec::new();
        d.extend_from_slice(&seq.to_le_bytes());
        d.extend_from_slice(&ts.to_le_bytes());
        d.extend_from_slice(&x.to_le_bytes());
        d.extend_from_slice(&y.to_le_bytes());
        d.extend_from_slice(&z.to_le_bytes());
        p.execute(CMD_RING_PUSH, &d)
    }

    #[test]
    fn record_is_62_bytes_and_parses_back() {
        let r = encode_record(10, 1234, -42, 17, -3).expect("fits");
        assert_eq!(r.len(), 62);
        assert_eq!(r[61], b'\n');
        assert_eq!(parse_seq_field(&r[..W_SEQ]).unwrap(), 10);
    }

    #[test]
    fn segment_name_round_trips() {
        assert_eq!(segment_name(42), "log0042.csv");
        assert_eq!(parse_segment_name("log0042.csv"), Some(42));
        assert_eq!(parse_segment_name("log0001.bin"), None);
    }

    #[test]
    fn init_on_empty_card_starts_at_seq_zero() {
        let mut p = SdCard::new(MemFs::new(), 7);
        let PluginResult::Ok(resp) = p.execute(CMD_INIT, &[]) else { panic!() };
        assert_eq!(resp.as_slice(), &0u32.to_le_bytes());
    }

    #[test]
    fn ring_push_appends_records() {
        let mut p = SdCard::new(MemFs::new(), 7);
        p.execute(CMD_INIT, &[]);
        for i in 0..3 {
            assert!(matches!(push(&mut p, i, i * 10, 1, 2, 3), PluginResult::Ok(_)));
        }
        assert_eq!(p.fs.size("log0001.csv").unwrap(), 3 * RECORD_BYTES);
    }

    #[test]
    fn ring_push_before_init_errors() {
        let mut p = SdCard::new(MemFs::new(), 7);
        let PluginResult::Error(e) = push(&mut p, 0, 0, 0, 0, 0) else { panic!() };
        assert_eq!(e.code, ERR_NOT_INIT);
    }

    #[test]
    fn ring_rotates_at_segment_limit_and_evicts_oldest() {
        // 2-record segments, depth 2 → after enough pushes we keep ≤ 2
        // segments and the current segment advances.
        let mut p = SdCard::with_limits(MemFs::new(), 7, 2 * RECORD_BYTES, 2);
        p.execute(CMD_INIT, &[]);
        for i in 0..8 {
            assert!(matches!(push(&mut p, i, 0, 0, 0, 0), PluginResult::Ok(_)));
        }
        let segs = p.sorted_segments().unwrap();
        assert!(segs.len() <= 2, "ring depth not enforced: {segs:?}");
        assert!(p.current_num >= 4, "expected several rotations, at {}", p.current_num);
    }

    #[test]
    fn boot_recovery_recovers_tail_seq() {
        let fs = MemFs::new();
        let mut p = SdCard::new(fs, 7);
        p.execute(CMD_INIT, &[]);
        for i in 0..5 {
            push(&mut p, 100 + i, 0, 0, 0, 0);
        }
        // Re-open against the same backing store → tail_seq recovered.
        let fs2 = core::mem::replace(&mut p.fs, MemFs::new());
        let mut p2 = SdCard::new(fs2, 7);
        let PluginResult::Ok(resp) = p2.execute(CMD_INIT, &[]) else { panic!() };
        assert_eq!(resp.as_slice(), &104u32.to_le_bytes());
    }

    #[test]
    fn ring_pop_frees_acked_segments_but_keeps_current() {
        let mut p = SdCard::with_limits(MemFs::new(), 7, 2 * RECORD_BYTES, 12);
        p.execute(CMD_INIT, &[]);
        // seqs 0..6 → segments log0001 (0,1), log0002 (2,3), log0003 (4,5), current log0004.
        for i in 0..6 {
            push(&mut p, i, 0, 0, 0, 0);
        }
        let before = p.sorted_segments().unwrap().len();
        // Ack through seq 3 → log0001 + log0002 freeable, log0003 (last seq 5) kept.
        let PluginResult::Ok(resp) = p.execute(CMD_RING_POP, &3u32.to_le_bytes()) else { panic!() };
        assert_eq!(resp.as_slice(), &2u32.to_le_bytes());
        let after = p.sorted_segments().unwrap().len();
        assert_eq!(before - after, 2);
    }

    #[test]
    fn file_open_write_close_list_get_delete_roundtrip() {
        let mut p = SdCard::new(MemFs::new(), 7);
        p.execute(CMD_INIT, &[]);

        assert!(matches!(p.execute(CMD_FILE_OPEN, b"capture-1.csv"), PluginResult::Ok(_)));
        let PluginResult::Ok(w) = p.execute(CMD_FILE_WRITE, b"hello ") else { panic!() };
        assert_eq!(w.as_slice(), &6u32.to_le_bytes());
        p.execute(CMD_FILE_WRITE, b"world");
        assert!(matches!(p.execute(CMD_FILE_CLOSE, &[]), PluginResult::Ok(_)));

        // list contains the file
        let PluginResult::Ok(l) = p.execute(CMD_FILE_LIST, &[]) else { panic!() };
        let listing = core::str::from_utf8(l.as_slice()).unwrap();
        assert!(listing.split('\n').any(|n| n == "capture-1.csv"));

        // get the whole file (offset 0, len 32)
        let mut get = Vec::new();
        get.extend_from_slice(&0u32.to_le_bytes());
        get.extend_from_slice(&32u16.to_le_bytes());
        get.extend_from_slice(b"capture-1.csv");
        let PluginResult::Ok(g) = p.execute(CMD_FILE_GET, &get) else { panic!() };
        assert_eq!(g.as_slice(), b"hello world");

        // delete it
        assert!(matches!(p.execute(CMD_FILE_DELETE, b"capture-1.csv"), PluginResult::Ok(_)));
        assert!(matches!(p.execute(CMD_FILE_DELETE, b"capture-1.csv"), PluginResult::Error(_)));
    }

    #[test]
    fn file_get_chunked_offset() {
        let mut p = SdCard::new(MemFs::new(), 7);
        p.execute(CMD_INIT, &[]);
        p.execute(CMD_FILE_OPEN, b"f.bin");
        p.execute(CMD_FILE_WRITE, b"0123456789");
        p.execute(CMD_FILE_CLOSE, &[]);
        // offset 4, len 3 → "456"
        let mut get = Vec::new();
        get.extend_from_slice(&4u32.to_le_bytes());
        get.extend_from_slice(&3u16.to_le_bytes());
        get.extend_from_slice(b"f.bin");
        let PluginResult::Ok(g) = p.execute(CMD_FILE_GET, &get) else { panic!() };
        assert_eq!(g.as_slice(), b"456");
    }

    #[test]
    fn file_write_with_no_open_file_errors() {
        let mut p = SdCard::new(MemFs::new(), 7);
        p.execute(CMD_INIT, &[]);
        let PluginResult::Error(e) = p.execute(CMD_FILE_WRITE, b"x") else { panic!() };
        assert_eq!(e.code, ERR_NO_OPEN_FILE);
    }

    #[test]
    fn sync_calls_backend() {
        let mut p = SdCard::new(MemFs::new(), 7);
        p.execute(CMD_INIT, &[]);
        assert!(matches!(p.execute(CMD_SYNC, &[]), PluginResult::Ok(_)));
        assert!(p.fs.syncs >= 1);
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = SdCard::new(MemFs::new(), 7);
        let PluginResult::Error(e) = p.execute(0xBB, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}
