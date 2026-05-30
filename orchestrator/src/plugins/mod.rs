//! Plugins performed by the orchestrator hive.
//!
//! Per [[feedback-sentants-vs-plugins-terminology]] in memory: plugins
//! do the imperative work (subprocess spawn, cargo, file I/O, network).
//! Sentants on top route events to plugins via `Action::PluginCall`.
//!
//! Phase 1.7b lands the first plugin — `claude-code` (subprocess driver
//! for `claude -p '<brief>' --output-format=stream-json`). Phase 1.7c+
//! adds cargo-runner, flasher, ota-push, webfetch, git-runner, sync,
//! catalogue, apiary, keyholder per SPEC-R2-COMPILER §3.3.

pub mod claude_code;

pub use claude_code::ClaudeCodePlugin;
