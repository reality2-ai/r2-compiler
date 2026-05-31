//! `claude-code` plugin — subprocess driver for `claude -p '<brief>'
//! `--output-format=stream-json`.
//!
//! Phase 1.7b — the first real plugin in the orchestrator hive.
//! Validates conjecture C-1 from `plan/PLAN.md`: that we can drive
//! a Claude Code subprocess autonomously, parse its `stream-json`
//! output line-by-line, and surface progress events on the R2 bus.
//!
//! ## Lifecycle
//!
//! - `Plugin::execute(CMD_START, brief)` — spawn `claude -p` in a
//!   worker thread; write the brief bytes to stdin; close stdin; read
//!   stdout line-by-line; push each parsed line (or raw text) through
//!   an internal channel; signal completion when the child exits.
//!   Returns `Ok(empty)` immediately — the actual progress is async.
//! - `Plugin::execute(CMD_CANCEL, _)` — kill the running child (if any).
//! - `Plugin::poll()` — drains one queued event from the worker channel
//!   and returns it as `(event_hash, payload)`. The bus emits it as a
//!   normal `QueuedEvent` and routes it to subscribed sentants
//!   (typically the Builder, which forwards to /r2).
//!
//! ## Events emitted (by hash, via `poll()`)
//!
//! - `r2.composer.build.progress` — one per parsed stream-json line.
//!   Payload is `{ "phase": "claude", "kind": "<type>", "line": "<raw json>" }`
//!   (the `kind` field comes from the stream-json's `type` field when
//!   present; otherwise it's `"unknown"`).
//! - `r2.composer.build.done` — the subprocess exited with code 0.
//!   Payload is `{ "exit_code": 0 }`.
//! - `r2.composer.build.error` — non-zero exit OR a stream-json parse
//!   error OR an IO error spawning the subprocess.
//!   Payload is `{ "exit_code": N, "message": "<reason>" }`.
//!
//! ## Testability
//!
//! `ClaudeCodePlugin::with_command` takes an arbitrary `Command`,
//! defaulting to `claude -p '--output-format=stream-json'`. Tests use
//! `echo` / `printf` with pre-recorded stream-json fixtures.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;

/// Side-channel slot for delivering a brief from a sentant to this
/// plugin out-of-band of the bus's `Action::PluginCall { data }`, which
/// caps at `MAX_ACTION_PAYLOAD = 256` bytes — too small for a real
/// authoring brief (system prompt + canvas state + chat history + user
/// message). The sentant `lock().unwrap().replace(brief)` before firing
/// PluginCall; the plugin's `start()` does `lock().unwrap().take()` to
/// consume it. Same engine thread, no contention.
pub type BriefSlot = Arc<Mutex<Option<String>>>;

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::Serialize;

/// Command opcode: start a new `claude -p` session.
pub const CMD_START: PluginCommand = 0x01;
/// Command opcode: cancel the running session (kills the child).
pub const CMD_CANCEL: PluginCommand = 0x02;

/// Error: subprocess spawn failed.
pub const ERR_SPAWN: u8 = 0x01;
/// Error: session is already running — refuse to start another.
pub const ERR_BUSY: u8 = 0x02;
/// Error: command byte was not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// One parsed event delivered from the worker thread to `poll()`.
#[derive(Debug)]
enum WorkerMsg {
    /// A text chunk extracted from an assistant / result line. Already
    /// sized to fit in r2-engine's 256B payload cap once packed into
    /// `{"kind":..., "text":...}`. The reader thread splits long text
    /// into multiple `Text` messages; the chat pane concatenates them
    /// back together as they arrive.
    Text { kind: String, chunk: String },
    /// Structural / metadata line (system init, rate_limit_event,
    /// tool_use, …) with no user-visible text. Emitted once so build
    /// consoles can observe the cadence; the chat pane skips them.
    Meta { kind: String },
    /// Subprocess finished with this exit code.
    Done { exit_code: i32 },
    /// Setup or read error.
    Error { message: String, exit_code: i32 },
}

/// Per-emission payload budget. r2-engine's `MAX_QUEUED_PAYLOAD` is 256
/// bytes; a `{"kind":"assistant","text":"…"}` wrapper costs ~28 bytes of
/// overhead, and JSON escaping inflates text by up to 2× in the
/// pathological case. 150 keeps us safely under the cap across the
/// expected character mix; r2-engine task #29 will lift this constraint.
const TEXT_CHUNK_BYTES: usize = 150;

/// Split a UTF-8 text into ≤`max` byte chunks, respecting char
/// boundaries so we never bisect a multi-byte codepoint.
fn chunk_text(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + max).min(text.len());
        // Walk back to a char boundary if we landed mid-codepoint.
        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            break; // shouldn't happen with valid UTF-8 + non-zero max
        }
        out.push(text[start..end].to_string());
        start = end;
    }
    out
}

/// Plugin instance — holds a receiver from the worker thread + an
/// optional `Child` handle for cancellation.
pub struct ClaudeCodePlugin {
    id: PluginId,
    rx: Option<mpsc::Receiver<WorkerMsg>>,
    child: Option<Child>,
    /// Pre-hashed event names — configurable per instance so the same
    /// plugin implementation drives both the build flow
    /// (`r2.composer.build.*`) and the author / chat flow
    /// (`r2.composer.author.*`).
    hash_text: u32,
    hash_done: u32,
    hash_error: u32,
    /// Reusable output buffer for `poll()`.
    out_buf: Vec<u8>,
    /// Command + args to spawn. Default = `claude -p --output-format=stream-json`.
    /// Tests override with `echo` / `printf` against fixtures.
    command_program: String,
    command_args: Vec<String>,
    /// Out-of-band brief slot for large prompts. See [`BriefSlot`].
    brief_slot: Option<BriefSlot>,
}

impl ClaudeCodePlugin {
    /// Construct for the **build flow** — emits `r2.composer.build.progress`,
    /// `r2.composer.build.done`, `r2.composer.build.error`.
    pub fn new(id: PluginId) -> Self {
        Self::with_events(
            id,
            "claude",
            vec![
                "-p".to_string(),
                "--output-format=stream-json".to_string(),
                "--verbose".to_string(),
            ],
            "r2.composer.build.progress",
            "r2.composer.build.done",
            "r2.composer.build.error",
        )
    }

    /// Construct for the **author / chat flow** — emits
    /// `r2.composer.author.reply`, `r2.composer.author.done`,
    /// `r2.composer.author.error`. Same subprocess shape; different
    /// event names so the webapp can route the stream into the chat
    /// pane rather than the build console.
    ///
    /// `brief_slot` is the out-of-band slot the `AuthorSentant` writes
    /// the prompt into; the bus's `Action::PluginCall { data }` is
    /// capped at `MAX_ACTION_PAYLOAD = 256` bytes, far too small for a
    /// real authoring brief (system prompt + canvas state + chat
    /// history + user message — kilobytes).
    pub fn new_author(id: PluginId, brief_slot: BriefSlot) -> Self {
        let mut p = Self::with_events(
            id,
            "claude",
            vec![
                "-p".to_string(),
                "--output-format=stream-json".to_string(),
                "--verbose".to_string(),
            ],
            "r2.composer.author.reply",
            "r2.composer.author.done",
            "r2.composer.author.error",
        );
        p.brief_slot = Some(brief_slot);
        p
    }

    /// Construct with a custom command (used by tests with fixture scripts).
    /// Defaults to the build event-name set.
    pub fn with_command(id: PluginId, program: impl Into<String>, args: Vec<String>) -> Self {
        Self::with_events(
            id,
            program,
            args,
            "r2.composer.build.progress",
            "r2.composer.build.done",
            "r2.composer.build.error",
        )
    }

    /// Construct with custom command + custom event names. The most
    /// general constructor; the typed builders above call it.
    pub fn with_events(
        id: PluginId,
        program: impl Into<String>,
        args: Vec<String>,
        text_event: &str,
        done_event: &str,
        error_event: &str,
    ) -> Self {
        Self {
            id,
            rx: None,
            child: None,
            hash_text:  r2_fnv::fnv1a_32(text_event.as_bytes()),
            hash_done:  r2_fnv::fnv1a_32(done_event.as_bytes()),
            hash_error: r2_fnv::fnv1a_32(error_event.as_bytes()),
            out_buf: Vec::with_capacity(256),
            command_program: program.into(),
            command_args: args,
            brief_slot: None,
        }
    }

    fn start(&mut self, data: &[u8]) -> PluginResult {
        if self.rx.is_some() {
            return PluginResult::Error(PluginError::new(ERR_BUSY, "session already running"));
        }
        // Pull the brief out of the side-channel slot when configured
        // (the chat / author flow uses this — bus payload is capped at
        // 256B, too small for real briefs). Fall back to the bus
        // payload for the build flow which still fits.
        let brief: Vec<u8> = self
            .brief_slot
            .as_ref()
            .and_then(|slot| slot.lock().unwrap().take())
            .map(String::into_bytes)
            .unwrap_or_else(|| data.to_vec());
        let brief = brief.as_slice();
        let mut cmd = Command::new(&self.command_program);
        cmd.args(&self.command_args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return PluginResult::Error(PluginError::new(
                    ERR_SPAWN,
                    &format!("spawn failed: {e}"),
                ));
            }
        };
        let stdin = child.stdin.take();
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                return PluginResult::Error(PluginError::new(ERR_SPAWN, "no stdout pipe"));
            }
        };

        let (tx, rx) = mpsc::channel();
        let brief_owned = brief.to_vec();

        // Writer thread: send the brief to claude's stdin.
        if let Some(mut sin) = stdin {
            let tx_err = tx.clone();
            thread::spawn(move || {
                if let Err(e) = sin.write_all(&brief_owned) {
                    let _ = tx_err.send(WorkerMsg::Error {
                        message: format!("stdin write failed: {e}"),
                        exit_code: -1,
                    });
                }
                // Closing `sin` here signals EOF to the child.
            });
        }

        // Reader thread: parse stdout line-by-line; extract text content
        // from assistant / result lines and chunk it before queueing.
        // Non-text lines (system init, rate_limit_event, tool_use, …)
        // surface as Meta so the build console can observe the cadence
        // without flooding the chat.
        let tx_rd = tx.clone();
        let reader_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(s) => {
                        let kind = parse_kind(&s);
                        match extract_text_content(&s) {
                            Some(text) if !text.is_empty() => {
                                for chunk in chunk_text(&text, TEXT_CHUNK_BYTES) {
                                    let _ = tx_rd.send(WorkerMsg::Text {
                                        kind: kind.clone(),
                                        chunk,
                                    });
                                }
                            }
                            _ => {
                                let _ = tx_rd.send(WorkerMsg::Meta { kind });
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx_rd.send(WorkerMsg::Error {
                            message: format!("stdout read failed: {e}"),
                            exit_code: -1,
                        });
                        break;
                    }
                }
            }
            // Reader thread ends when the child closes its stdout. The
            // wait thread below joins on this handle before sending the
            // terminal Done/Error so Meta/Text events can never race
            // behind Done into the channel.
        });

        // Wait thread: drain the child, join the reader, THEN report exit.
        thread::spawn(move || {
            let status = child.wait();
            // CRITICAL: join the reader before signalling completion.
            // Without this, fast-exiting children (printf in tests; one-
            // shot tool invocations in prod) emit Done into the channel
            // BEFORE the reader has finished draining stdout. The poll
            // loop then early-exits on Done and the trailing Meta/Text
            // events are never observed.
            let _ = reader_handle.join();
            match status {
                Ok(s) => {
                    let code = s.code().unwrap_or(-1);
                    if code == 0 {
                        let _ = tx.send(WorkerMsg::Done { exit_code: 0 });
                    } else {
                        let _ = tx.send(WorkerMsg::Error {
                            message: format!("claude exited with code {code}"),
                            exit_code: code,
                        });
                    }
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Error {
                        message: format!("wait failed: {e}"),
                        exit_code: -1,
                    });
                }
            }
        });

        self.rx = Some(rx);
        // We don't keep the Child here — wait thread owns it.
        // CMD_CANCEL would need a separate path; v0.1 returns ERR_BUSY
        // and relies on the wait thread exiting naturally.
        self.child = None;
        PluginResult::Ok(PluginResponse::empty())
    }

    fn cancel(&mut self) -> PluginResult {
        // Phase 1.7b stub: drop the receiver — Done/Error events from the
        // wait thread fall on the floor. Real cancel needs the Child
        // handle held here, which means refactoring the wait thread to
        // share it via an Arc<Mutex<>>. Tracked as TODO; not blocking.
        self.rx = None;
        self.child = None;
        PluginResult::Ok(PluginResponse::empty())
    }

    /// Pack a JSON payload into the reusable output buffer + return its slice.
    fn pack<T: Serialize>(&mut self, v: &T) -> &[u8] {
        self.out_buf.clear();
        if let Ok(json) = serde_json::to_writer(&mut self.out_buf, v) {
            let _ = json;
        }
        &self.out_buf
    }
}

impl Plugin for ClaudeCodePlugin {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_START => self.start(data),
            CMD_CANCEL => self.cancel(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command byte")),
        }
    }

    fn name(&self) -> &str { "claude-code" }

    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        // Drain one message from the worker channel.
        let msg = self.rx.as_ref()?.try_recv().ok()?;
        // Hashes are Copy — read them BEFORE borrowing self for pack().
        let (hash_text, hash_done, hash_error) =
            (self.hash_text, self.hash_done, self.hash_error);
        match msg {
            WorkerMsg::Text { kind, chunk } => {
                let payload = self.pack(&serde_json::json!({
                    "kind": kind,
                    "text": chunk,
                }));
                Some((hash_text, payload))
            }
            WorkerMsg::Meta { kind } => {
                let payload = self.pack(&serde_json::json!({
                    "kind": kind,
                    "text": serde_json::Value::Null,
                }));
                Some((hash_text, payload))
            }
            WorkerMsg::Done { exit_code } => {
                self.rx = None;
                let payload = self.pack(&serde_json::json!({ "exit_code": exit_code }));
                Some((hash_done, payload))
            }
            WorkerMsg::Error { message, exit_code } => {
                self.rx = None;
                let payload = self.pack(&serde_json::json!({
                    "exit_code": exit_code,
                    "message": message,
                }));
                Some((hash_error, payload))
            }
        }
    }
}

/// Parse the `"type"` field from a stream-json line. Returns `"unknown"`
/// for non-JSON or shapeless lines.
fn parse_kind(line: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
            return t.to_string();
        }
    }
    "unknown".to_string()
}

/// Extract the human-readable text content from a Claude Code stream-json
/// line. Returns `None` for system / init / tool-use / result / other
/// lines that carry no NEW user-visible text — the chat pane skips
/// those; the build console still observes the cadence via Meta events.
///
/// Result lines are deliberately treated as Meta: their `result` field
/// duplicates the assistant text already emitted, and showing it again
/// would render the reply twice in chat.
///
/// Recognised text-bearing shapes:
///   `{"type":"assistant","message":{"content":[{"type":"text","text":"…"},…]}}`
fn extract_text_content(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    match v.get("type").and_then(|t| t.as_str())? {
        "assistant" => {
            let content = v.get("message")?.get("content")?.as_array()?;
            let mut out = String::new();
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                        out.push_str(t);
                    }
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

/// Static instance of the orchestrator's claude-code plugin slot's
/// pre-hashed names — handy for tests that need to identify which
/// event came out of `poll()`.
pub fn hashes() -> &'static (u32, u32, u32) {
    static H: OnceLock<(u32, u32, u32)> = OnceLock::new();
    H.get_or_init(|| (
        r2_fnv::fnv1a_32(b"r2.composer.build.progress"),
        r2_fnv::fnv1a_32(b"r2.composer.build.done"),
        r2_fnv::fnv1a_32(b"r2.composer.build.error"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn drain_with_timeout(p: &mut ClaudeCodePlugin, timeout: Duration) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some((h, payload)) = p.poll() {
                out.push((h, payload.to_vec()));
                continue;
            }
            std::thread::sleep(Duration::from_millis(20));
            // Stop when we've seen a terminal Done/Error event.
            if let Some((h, _)) = out.last() {
                let (_, done, err) = *hashes();
                if *h == done || *h == err {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = ClaudeCodePlugin::new(0);
        let r = p.execute(0xAA, &[]);
        match r {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_UNKNOWN_COMMAND),
            _ => panic!("expected ERR_UNKNOWN_COMMAND"),
        }
    }

    #[test]
    fn echo_fixture_round_trips_to_progress_then_done() {
        // Use printf to emit two stream-json-ish lines then exit 0.
        // The shell-quoting matters; use a script via `sh -c`.
        let mut p = ClaudeCodePlugin::with_command(
            0,
            "sh",
            vec![
                "-c".into(),
                r#"printf '{"type":"system","subtype":"init"}\n{"type":"assistant","message":"hello"}\n'"#.into(),
            ],
        );

        match p.execute(CMD_START, &[]) {
            PluginResult::Ok(_) => {}
            PluginResult::Error(e) => panic!("start failed: code 0x{:02X}: {}", e.code, e.description()),
        }

        let events = drain_with_timeout(&mut p, Duration::from_secs(3));
        let (progress, done, _err) = *hashes();
        let prog_count = events.iter().filter(|(h, _)| *h == progress).count();
        let done_count = events.iter().filter(|(h, _)| *h == done).count();

        assert!(prog_count >= 2, "expected >=2 progress events, got {prog_count}: {events:?}");
        assert_eq!(done_count, 1, "expected 1 done event");

        // First progress event's kind should be "system" (Meta variant
        // since the line carries no text content).
        let first_progress = events.iter().find(|(h, _)| *h == progress).expect("a progress event");
        let parsed: serde_json::Value = serde_json::from_slice(&first_progress.1).unwrap();
        assert_eq!(parsed["kind"], "system");
        assert!(parsed["text"].is_null());
    }

    #[test]
    fn nonzero_exit_yields_error_event() {
        let mut p = ClaudeCodePlugin::with_command(
            0,
            "sh",
            vec!["-c".into(), "exit 3".into()],
        );
        match p.execute(CMD_START, &[]) {
            PluginResult::Ok(_) => {}
            PluginResult::Error(e) => panic!("start failed: 0x{:02X}", e.code),
        }
        let events = drain_with_timeout(&mut p, Duration::from_secs(2));
        let (_, _, err) = *hashes();
        let err_count = events.iter().filter(|(h, _)| *h == err).count();
        assert_eq!(err_count, 1, "expected 1 error event for nonzero exit, got {events:?}");
        let payload: serde_json::Value =
            serde_json::from_slice(&events.iter().find(|(h, _)| *h == err).unwrap().1).unwrap();
        assert_eq!(payload["exit_code"], 3);
    }

    #[test]
    fn cannot_start_twice() {
        // Spawn a long-running command so the first session lingers.
        let mut p = ClaudeCodePlugin::with_command(
            0,
            "sh",
            vec!["-c".into(), "sleep 5".into()],
        );
        assert!(matches!(p.execute(CMD_START, &[]), PluginResult::Ok(_)));
        match p.execute(CMD_START, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_BUSY),
            _ => panic!("expected ERR_BUSY for double-start"),
        }
        // Cancel to let the test exit promptly.
        p.execute(CMD_CANCEL, &[]);
    }

    #[test]
    fn spawn_failure_returns_err_spawn() {
        let mut p = ClaudeCodePlugin::with_command(0, "this-binary-does-not-exist-r2", vec![]);
        match p.execute(CMD_START, &[]) {
            PluginResult::Error(e) => assert_eq!(e.code, ERR_SPAWN),
            _ => panic!("expected ERR_SPAWN"),
        }
    }
}
