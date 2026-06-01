//! Plugins performed by the orchestrator hive.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] in memory: plugins
//! do the imperative work (subprocess spawn, cargo, file I/O, network).
//! Sentants on top route events to plugins via `Action::PluginCall`.
//!
//! Phase 1.7b lands the first plugin — `claude-code` (subprocess driver
//! for `claude -p '<brief>' --output-format=stream-json`). Phase 1.7c+
//! adds cargo-runner, flasher, ota-push, webfetch, git-runner, sync,
//! catalogue, apiary, keyholder per SPEC-R2-COMPOSER §3.3.

pub mod claude_code;
pub mod flasher;
pub mod keyholder;
pub mod provision;
pub mod usb_watcher;

pub use claude_code::ClaudeCodePlugin;
pub use flasher::{FlashParams, FlashRegion, FlasherPlugin, FlasherSlot};
pub use keyholder::{KeyholderPlugin, KeyholderSlot, SignCertRequest};
pub use provision::{ComposeOfferRequest, ProvisionPlugin, ProvisionSlot};
pub use usb_watcher::{UsbPort, UsbSnapshot, UsbWatcherPlugin};
