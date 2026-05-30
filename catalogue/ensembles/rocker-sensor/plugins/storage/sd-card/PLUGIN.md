# sd-card

**Version:** 0.1.0 (metadata draft) · **Modes:** `aot` only (esp32-s3) · **Conformance:** R2-PLUGIN §12

## 1. Purpose

FATFS-mounted microSD on a shared SPI bus + the 20-byte fixed-record acceleration ring (r2-workshop SPEC-R2-WORKSHOP-CAPTURE §6). Two distinct functions:

- **Ring buffer** for store-and-forward — when WiFi is down or the dashboard lags, acceleration samples queue here; `Recorder` sentant drains on reconnect.
- **Capture files** — operator-named session recordings (`<ts16>-<name>.csv`) with long filenames (FATFS LFN enabled in sdkconfig).

## 2. Modes & Platforms

| Mode | Targets |
|---|---|
| `aot` | `esp32-s3` (the dfr1117 has only 4 MB flash, no internal FAT, and uses external SD too — but the wiring + driver are S3-specific in r2-workshop today) |

## 3. Events Handled

| Event | Parameters | Purpose |
|---|---|---|
| `r2.hw.sd.init` | `{ cs_pin, cd_pin? }` | Mount FATFS |
| `r2.hw.sd.ring.push` | `{ record: [u8; 20] }` | Append acceleration record (seq + ts + x/y/z) |
| `r2.hw.sd.ring.pop` | `{ through_seq: u32 }` | Free records up to seq N (called on cumulative ack) |
| `r2.hw.sd.file.open` | `{ name: str, mode: "w" }` | Open a capture file |
| `r2.hw.sd.file.write` | `{ handle, data }` | Append bytes |
| `r2.hw.sd.file.close` | `{ handle }` | Close + fsync |
| `r2.hw.sd.file.list` | `{}` | List files in /captures/ |
| `r2.hw.sd.file.get` | `{ name, offset?, len? }` | Read |
| `r2.hw.sd.file.delete` | `{ name }` | Remove |
| `r2.hw.sd.sync` | `{}` | fsync the FATFS |

## 4. Events Emitted

| Status | Data | Notes |
|---|---|---|
| `"ok"` (init) | `{ mounted: true, free_kb: u32 }` | |
| `"ok"` (file.list) | `{ files: [{ name, size, ts }] }` | |
| `"ok"` (file.get) | `{ data: [u8] }` | |
| `"error"` | `error: "mount_failed"` / `"einval"` / `"enospc"` / ... | |

## 5. Configuration

```yaml
data:
  cs_pin: 9              # devkitc default; varies per carrier
  cd_pin: 15             # optional
  mount_point: "/sdcard"
  ring_capacity: 100000  # records (20 B each = 2 MB worst case)
```

## 6. Example Sentants

[`Recorder`](../../../sentants/Recorder/) (ring) and [`Capture`](../../../sentants/Capture/) (named files).

## 7. Hardware / Host Requirements

- microSD breakout, SPI, 3.3 V tolerant
- Shared SPI bus with the accel plugin — different CS lines (CS=GPIO9 on devkitc; CS=D4/GPIO5 on xiao; CS=GPIO7 on dfr1117)
- 10 kΩ pull-up on SD CS (most breakouts include it)
- ≥4 GB Class-10 card
- FATFS_LFN_HEAP=y in sdkconfig.defaults (set on all three carriers)

## 8. Credentials

None.

## 9. Known Limitations

- **Source not yet extracted** — `r2-workshop/firmware/esp32-s3/{devkitc,xiao}/src/ring.rs` + `sd.rs` are the reference.
- **dfr1117 support not declared** — the C6 firmware doesn't currently use this plugin (capture data still lives in `out/`; SD wiring is on the carrier but driver port pending).
- **No directory-tree support** — single flat `/sdcard/captures/` directory.
- **Ring records fixed at 20 bytes** — `(seq:u32, ts_ms:u32, x:i32, y:i32, z:i32)`. Schema change is a wire-breaking event.

## 10. Changelog

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1.0 | Metadata draft. |
