//! r2-composer-specific authoring scaffolding — host-OS subprocess
//! wrappers, sysfs watchers, and AI orchestration. These exist ONLY
//! because r2-composer is an authoring tool; they have no analogue in
//! a generic R2 hive and no R2-* spec governs them.
//!
//! Distinct from [[crate::substrate]] which holds R2 protocol-role
//! implementations (L5 Trust, L2 Discovery, …) that any R2 hive could
//! have. Components here are "above" the substrate line but are NOT
//! R2-PLUGIN-spec plugins either — they're closer to the host-OS
//! shell that lets the composer drive external tools.
//!
//! Members:
//!
//! - `claude_code` — drives the `claude -p` subprocess for chat + author flows.
//! - `flasher`     — wraps `esptool` for USB first-install.
//! - `usb_watcher` — polls `/sys/class/tty/` for newly-attached serial devices.
//!
//! Future:
//! - `cargo_runner`     — wraps `cargo build` for per-target compile (SPEC-APIARY-COMPOSE §6)
//! - `git_runner`       — wraps `git` for apiary repo operations
//! - `gh_runner`        — wraps `gh` for apiary publish flow
//! - `catalogue_server` — manages catalogue/ file IO

pub mod claude_code;
pub mod flasher;
pub mod usb_watcher;

pub use claude_code::ClaudeCodePlugin;
pub use flasher::{FlashParams, FlashRegion, FlasherPlugin, FlasherSlot};
pub use usb_watcher::{UsbPort, UsbSnapshot, UsbWatcherPlugin};
