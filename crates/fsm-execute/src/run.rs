//! Subprocess execution: what one handler run captured, and what it did.
//!
//! The runner spawns, reaps, and kills handler processes and reports what
//! happened; the [`Pipeline`] in `run/pipeline.rs` maps that outcome onto
//! journaled reality through the store's own idempotent mutators. The split is
//! deliberate and now physical: the runner owns no policy, and the pipeline
//! spawns nothing.
//!
//! Two kinds of run live here, and they differ in exactly one way — how the
//! answer arrives. A process handler's answer is its exit status, read by
//! `try_wait`; an MCP handler's is one tool call's result, read from the worker
//! that holds its pipes. Everything else is shared: one child per effect, one
//! timeout, one kill path, one bounded capture, one `Drop`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use fsm_core::json::Value;
use fsm_core::sha256::{Sha256, to_hex};

use crate::error::ExecError;
use crate::mcp_client::{McpOutcome, ProtocolFault, converse};

mod pipeline;

pub use pipeline::{Pipeline, SettleOutcome};

/// Bytes of one captured stream that reach the journal.
pub const ACK_OUTPUT_CAP: usize = 4096;

/// Bytes of one captured stream the runner will read at all.
///
/// The cap above bounds what is *journaled*; this one bounds what is *read*,
/// because the digest is taken over the whole stream and a handler in a
/// `yes`-style loop can write a capture file faster than any timeout stops it.
/// Past this bound the capture is reported as truncated with no digest — the
/// honest statement that this is a prefix and the rest cannot be proved —
/// rather than hashing gigabytes on the tick thread.
pub const MAX_CAPTURE_READ_BYTES: usize = 1024 * 1024;

/// A capped capture of one output stream, digested when it overflows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedBytes {
    /// At most [`ACK_OUTPUT_CAP`] bytes from the front of the stream.
    pub bytes: Vec<u8>,
    /// Whether the stream was longer than the cap.
    pub truncated: bool,
    /// Hex SHA-256 of the *whole* stream, present only when truncated.
    pub sha256: Option<String>,
}

impl BoundedBytes {
    /// An empty capture, for a stream that produced nothing.
    pub fn empty() -> Self {
        Self {
            bytes: Vec::new(),
            truncated: false,
            sha256: None,
        }
    }

    /// Read a capture file, keeping the first [`ACK_OUTPUT_CAP`] bytes and
    /// digesting the whole stream when it is longer.
    ///
    /// The digest is what keeps a large output tamper-evident without storing
    /// it: journal records are permanent, and a handler that prints a megabyte
    /// must not put a megabyte in the chain. Mirrors SPEC §Payload size's
    /// "journal a digest" rule.
    ///
    /// A file that cannot be read yields an empty capture rather than an
    /// error. The ack has to be journaled either way — refusing to write it
    /// because a temporary file vanished would leave the effect pending
    /// forever, which is strictly worse than an ack with an empty `stdout`.
    /// A read that fails *part way* is marked `truncated` with no digest,
    /// which is this type's way of saying "this is a prefix, and I cannot
    /// prove what the rest was": presenting a partial capture as the whole
    /// output would put a falsehood in a permanent record.
    fn read_capped(path: &Path) -> Self {
        let Ok(mut file) = File::open(path) else {
            return Self::empty();
        };
        let mut bytes = Vec::new();
        let mut hasher = Sha256::new();
        let mut total = 0usize;
        let mut chunk = [0u8; 8192];
        let mut complete = true;
        loop {
            match file.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    hasher.update(&chunk[..read]);
                    total = total.saturating_add(read);
                    if bytes.len() < ACK_OUTPUT_CAP {
                        let room = ACK_OUTPUT_CAP - bytes.len();
                        bytes.extend_from_slice(&chunk[..read.min(room)]);
                    }
                    if total >= MAX_CAPTURE_READ_BYTES {
                        // A runaway handler must not turn one tick into a
                        // multi-gigabyte hash.
                        complete = false;
                        break;
                    }
                }
                Err(_) => {
                    complete = false;
                    break;
                }
            }
        }
        let over_cap = total > ACK_OUTPUT_CAP;
        Self {
            bytes,
            truncated: over_cap || !complete,
            sha256: (over_cap && complete).then(|| to_hex(&hasher.finalize())),
        }
    }

    /// Render the capture as a valid JSON string, lossily and on a character
    /// boundary.
    ///
    /// Handler output is arbitrary bytes and a record body is canonical JSON;
    /// this is the one conversion between them, and it must never fail. A
    /// multi-byte character straddling the cap is dropped rather than rendered
    /// as a replacement character, because it is a *truncation* artefact
    /// rather than something the handler wrote; a genuinely invalid byte the
    /// handler did write survives as U+FFFD.
    pub fn to_json_string(&self) -> String {
        String::from_utf8_lossy(without_partial_tail(&self.bytes)).into_owned()
    }
}

/// Drop a final UTF-8 sequence the cap cut in half.
///
/// Only the tail is inspected, and deliberately so: an invalid byte earlier in
/// the stream is something the handler wrote and stays as U+FFFD, while an
/// incomplete sequence at the very end is an artefact of where the capture
/// stopped and is not the handler's output at all.
fn without_partial_tail(bytes: &[u8]) -> &[u8] {
    let earliest_lead = bytes.len().saturating_sub(4);
    for index in (earliest_lead..bytes.len()).rev() {
        let byte = bytes[index];
        if byte & 0xC0 == 0x80 {
            continue; // a continuation byte: keep walking back to its lead
        }
        let needed = match byte {
            0x00..=0x7F => 1,
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            // Not a lead byte at all; lossy rendering turns it into U+FFFD.
            _ => 1,
        };
        return if bytes.len() - index < needed {
            &bytes[..index]
        } else {
            bytes
        };
    }
    bytes
}

/// Why an in-flight handler was killed. A timeout and a cancel are different
/// facts about the run, and the ack records which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillReason {
    /// The run passed its handler's `timeout_ms`.
    Timeout,
    /// The instance was cancelled while the run was in flight.
    Cancelled,
}

impl KillReason {
    /// The `exec/*` code this reason is journaled as.
    pub fn code(self) -> &'static str {
        match self {
            KillReason::Timeout => "exec/timeout",
            KillReason::Cancelled => "exec/cancelled",
        }
    }
}

/// The one tool call a `kind: "mcp"` handler makes, with its arguments
/// already substituted.
///
/// Built by the scheduler, which is where substitution lives for `argv` too:
/// the runner receives a finished call and never looks at an effect's
/// arguments, which is what keeps it unable to construct one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCall {
    /// The tool the operator's table named.
    pub tool: String,
    /// The substituted arguments object.
    pub arguments: Value,
}

/// What one handler run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The process exited on its own.
    Completed {
        /// Exit status, or `-1` for a process killed by a signal.
        status: i32,
        /// Captured standard output.
        stdout: BoundedBytes,
        /// Captured standard error.
        stderr: BoundedBytes,
    },
    /// The executor stopped it.
    Killed {
        /// Timeout or cancellation.
        reason: KillReason,
    },
    /// It never started.
    SpawnFailed {
        /// The command that could not be spawned.
        argv0: String,
    },
    /// It could not even be assembled — an argv the effect's own arguments
    /// cannot fill.
    ///
    /// Distinct from [`RunOutcome::SpawnFailed`] because the journal is
    /// permanent: recording `exec/spawn` and a command that was never chosen
    /// would tell a later reader the wrong story about a fault that is really
    /// in the handler table.
    NotStarted {
        /// The `exec/*` code for the fault.
        code: &'static str,
        /// What could not be resolved, in one identifier.
        detail: String,
    },
    /// A `kind: "mcp"` handler's one tool call, and what the server wrote to
    /// its standard error while making it.
    ///
    /// The capture is here for the same reason it is on a process run: a
    /// server that crashes mid-conversation must leave evidence rather than
    /// silence. It reaches the ack only on a failure, where it is the
    /// diagnosis; a successful call's ack carries the tool's answer, not the
    /// server's logs.
    Mcp {
        /// What the exchange did, at the protocol level.
        outcome: McpOutcome,
        /// The server's captured standard error.
        stderr: BoundedBytes,
    },
}

impl RunOutcome {
    /// Whether this outcome acks `ok`. Only a clean exit, or a tool that
    /// answered without raising its own error flag, does.
    pub fn succeeded(&self) -> bool {
        match self {
            RunOutcome::Completed { status: 0, .. } => true,
            RunOutcome::Mcp { outcome, .. } => outcome.succeeded(),
            _ => false,
        }
    }

    /// Which retry class this failure belongs to, or `None` when no policy
    /// may act on it.
    ///
    /// `None` is not "unclassified": it is the executor refusing to retry, and
    /// each case is a deliberate refusal rather than a gap.
    ///
    /// * A clean exit is not a failure at all.
    /// * A run killed because its **instance was cancelled** must never be
    ///   restarted. The cancellation is a decision somebody took about the
    ///   whole instance, and re-running the handler would spend the operator's
    ///   retry budget undoing it.
    /// * An argv the effect's own arguments cannot fill is a fault in the
    ///   handler table, not a transient failure of the world. The same
    ///   substitution against the same journaled args fails identically every
    ///   time, so a retry is a guaranteed waste of the budget.
    pub fn failure_class(&self) -> Option<&'static str> {
        match self {
            RunOutcome::Completed { status: 0, .. } => None,
            RunOutcome::Completed { .. } => Some("nonzero_exit"),
            RunOutcome::Killed {
                reason: KillReason::Timeout,
            } => Some("timeout"),
            RunOutcome::Killed {
                reason: KillReason::Cancelled,
            } => None,
            RunOutcome::SpawnFailed { .. } => Some("spawn"),
            RunOutcome::NotStarted { .. } => None,
            // A tool that failed and a server that answered with a JSON-RPC
            // error are both statements about the *call*, which the world may
            // answer differently next time. A protocol violation is not: the
            // server is broken, and running it again produces the same broken
            // exchange.
            RunOutcome::Mcp { outcome, .. } => match outcome {
                McpOutcome::Answered { is_error: true, .. } | McpOutcome::RpcError { .. } => {
                    Some("mcp_error")
                }
                McpOutcome::Answered { .. } | McpOutcome::Protocol(_) => None,
            },
        }
    }

    /// The deterministic `result` the ack is fingerprinted over.
    ///
    /// No timestamp, duration, or pid may enter it: the store keys idempotency
    /// on the content, so anything varying between the write and a later
    /// re-issue turns a replay into a conflict.
    pub fn ack_result(&self) -> Value {
        let mut result = BTreeMap::new();
        match self {
            RunOutcome::Completed {
                status,
                stdout,
                stderr,
            } => {
                result.insert("status".into(), Value::Num(status.to_string()));
                insert_stream(&mut result, "stdout", stdout);
                insert_stream(&mut result, "stderr", stderr);
            }
            RunOutcome::Killed { reason } => {
                result.insert("status".into(), Value::Num("-1".into()));
                result.insert("error".into(), Value::Str(reason.code().into()));
            }
            RunOutcome::SpawnFailed { argv0 } => {
                result.insert("status".into(), Value::Num("-1".into()));
                result.insert("error".into(), Value::Str("exec/spawn".into()));
                result.insert("argv0".into(), Value::Str(argv0.clone()));
            }
            RunOutcome::NotStarted { code, detail } => {
                result.insert("status".into(), Value::Num("-1".into()));
                result.insert("error".into(), Value::Str((*code).into()));
                result.insert("detail".into(), Value::Str(detail.clone()));
            }
            RunOutcome::Mcp { outcome, stderr } => return mcp_ack_result(outcome, stderr),
        }
        Value::Obj(result)
    }

    /// The ack `result` for the failure that used up a handler's retry budget.
    ///
    /// The last run's capture is kept whole — an operator reading a dead
    /// letter wants the output of the attempt that finally gave up — and three
    /// keys are laid over it: `error`, which every failure path in this crate
    /// already uses for its cause, `attempts`, which names how many runs it
    /// took to get here, and `class`, which preserves the cause `error` was
    /// carrying before exhaustion took its place. Without `class` a timeout
    /// and a non-zero exit would be indistinguishable after the fact.
    ///
    /// Still fingerprint-safe: `attempts` is derived from the journal and
    /// `class` from the outcome, so a re-issued ack rebuilds the same bytes.
    pub fn exhausted_ack_result(&self, attempts: u32, class: &str) -> Value {
        let mut result = match self.ack_result() {
            Value::Obj(fields) => fields,
            _ => BTreeMap::new(),
        };
        result.insert(
            "error".into(),
            Value::Str(crate::error::RETRIES_EXHAUSTED.into()),
        );
        result.insert("attempts".into(), Value::Num(attempts.to_string()));
        result.insert("class".into(), Value::Str(class.into()));
        Value::Obj(result)
    }
}

/// A failure that used up its handler's retry budget.
///
/// Carried into the ack rather than decided there, because the count comes
/// from the journal and the class from the finished run — two facts the
/// writing half has no business re-deriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhaustion {
    /// Total attempts made, including the first.
    pub attempts: u32,
    /// The failure class the policy had been retrying.
    pub class: &'static str,
}

/// Map one tool call onto the ack's `result`.
///
/// Deterministic by construction: every field is either the server's own
/// answer or a constant from this crate. No timestamp, pid, duration, or
/// elapsed value may enter it — the store fingerprints the ack over this
/// object, and a re-issue whose content differs is a `req/request_id_conflict`
/// rather than a replay.
///
/// The server's standard error rides along **only on a failure**, where it is
/// the evidence an operator needs. A successful call's ack carries the tool's
/// answer; the server's logs would only crowd it.
///
/// `7703` extends this with the cap and digest a chatty tool needs.
fn mcp_ack_result(outcome: &McpOutcome, stderr: &BoundedBytes) -> Value {
    let mut result = BTreeMap::new();
    match outcome {
        McpOutcome::Answered {
            structured,
            is_error,
        } => {
            result.insert("structured".into(), structured.clone());
            if *is_error {
                result.insert("error".into(), Value::Str("mcp/tool_error".into()));
            }
        }
        McpOutcome::RpcError { code, message } => {
            result.insert("error".into(), Value::Str("mcp/rpc_error".into()));
            result.insert("code".into(), Value::Num(code.to_string()));
            result.insert("message".into(), Value::Str(message.clone()));
        }
        McpOutcome::Protocol(fault) => {
            result.insert("error".into(), Value::Str("exec/mcp_protocol".into()));
            result.insert("detail".into(), Value::Str(fault.as_str().into()));
        }
    }
    if !outcome.succeeded() {
        insert_stream(&mut result, "stderr", stderr);
    }
    Value::Obj(result)
}

/// Put one captured stream into the ack, with its digest when the capture is
/// only a prefix. The digest's presence is what tells a later reader that the
/// text is not the whole output.
fn insert_stream(result: &mut BTreeMap<String, Value>, name: &str, stream: &BoundedBytes) {
    result.insert(name.into(), Value::Str(stream.to_json_string()));
    if let Some(digest) = &stream.sha256 {
        result.insert(format!("{name}_sha256"), Value::Str(digest.clone()));
    }
}

/// One child, and whatever else is needed to learn what it did.
///
/// The two kinds differ in exactly one way — how the answer arrives — and
/// share everything else: the runner owns the `Child` in both, so one kill
/// path, one reap path, and one `Drop` cover them.
enum Running {
    /// A subprocess whose exit status is the answer.
    Process {
        child: Child,
        stdout_path: PathBuf,
        stderr_path: PathBuf,
    },
    /// An MCP server whose answer arrives from the worker holding its pipes.
    Mcp {
        child: Child,
        stderr_path: PathBuf,
        /// The worker's one message, once it has been taken off the channel.
        answer: Option<McpOutcome>,
        answers: Receiver<McpOutcome>,
    },
}

impl Running {
    fn child_mut(&mut self) -> &mut Child {
        match self {
            Running::Process { child, .. } | Running::Mcp { child, .. } => child,
        }
    }

    /// Take the worker's answer if it has arrived, and say whether one is now
    /// in hand.
    ///
    /// A worker that ended without sending — the one way a panic in the
    /// conversation could surface — is read as a closed stream rather than
    /// left in flight forever.
    fn collect(&mut self) -> bool {
        let Running::Mcp {
            answer, answers, ..
        } = self
        else {
            return false;
        };
        if answer.is_none() {
            match answers.try_recv() {
                Ok(received) => *answer = Some(received),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    *answer = Some(McpOutcome::Protocol(ProtocolFault::Closed));
                }
            }
        }
        answer.is_some()
    }

    /// Remove the capture files nothing will ever read.
    fn discard_captures(&self) {
        match self {
            Running::Process {
                stdout_path,
                stderr_path,
                ..
            } => {
                let _ = std::fs::remove_file(stdout_path);
                let _ = std::fs::remove_file(stderr_path);
            }
            Running::Mcp { stderr_path, .. } => {
                let _ = std::fs::remove_file(stderr_path);
            }
        }
    }
}

/// Distinguishes the scratch directories of two runners in one process, which
/// the chaos harness creates when it restarts an executor: a shared directory
/// would be removed by the first `Drop` out from under the second runner.
static RUNNER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Per-run capture file names, so no two runs can name the same file whatever
/// characters an instance id contains.
static SPAWN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The only component that spawns processes.
pub struct Runner {
    scratch: PathBuf,
    children: BTreeMap<String, Running>,
}

impl Runner {
    /// Create the capture directory this runner owns.
    ///
    /// Created exclusively, never adopted. The temporary directory is world
    /// writable on a Unix host, so a guessable name that `create_dir_all`
    /// accepts when it already exists would let a local account pre-create it
    /// — as a symlink, say — and read every handler's captured output, or have
    /// this process's `Drop` remove a directory of their choosing. An
    /// exclusive create refuses both, and on Unix the directory is made 0700
    /// so the captures are unreadable by anyone else even while they exist.
    pub fn new() -> Result<Self, ExecError> {
        let mut attempt = 0u32;
        loop {
            let scratch = std::env::temp_dir().join(format!(
                "fsm-exec-{}-{}-{}",
                std::process::id(),
                RUNNER_SEQUENCE.fetch_add(1, Ordering::Relaxed),
                unique_suffix(attempt)
            ));
            match private_directory().create(&scratch) {
                Ok(()) => {
                    return Ok(Self {
                        scratch,
                        children: BTreeMap::new(),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 8 => {
                    attempt += 1;
                }
                Err(error) => {
                    return Err(ExecError::new(
                        "exec/spawn",
                        format!(
                            "cannot create the capture directory {}: {error}",
                            scratch.display()
                        ),
                    )
                    .hint("point TMPDIR at a writable directory this user owns"));
                }
            }
        }
    }

    /// The directory this runner captures handler output into.
    pub fn scratch_dir(&self) -> &Path {
        &self.scratch
    }

    /// The effects this runner currently has a child for.
    pub fn running_effects(&self) -> Vec<String> {
        self.children.keys().cloned().collect()
    }

    /// The effects whose child has exited and is waiting to be collected.
    ///
    /// Separate from [`Runner::poll`] because the driver has to know whether a
    /// tick needs the writer *before* it takes the outcome: collecting an
    /// outcome it then cannot journal would throw away a completed run.
    /// `try_wait` remembers the exit status, so asking here and taking it
    /// afterwards reaps exactly once.
    pub fn finished_effects(&mut self) -> Vec<String> {
        let mut finished = Vec::new();
        for (effect_id, running) in &mut self.children {
            // An MCP run is over when the *conversation* is, not when the
            // server exits: a server that answers and then lingers has done
            // its job, and waiting for it to exit would hold the effect open
            // for as long as it chose.
            if running.collect() {
                finished.push(effect_id.clone());
                continue;
            }
            if matches!(running, Running::Mcp { .. }) {
                continue;
            }
            match running.child_mut().try_wait() {
                Ok(Some(_)) | Err(_) => finished.push(effect_id.clone()),
                Ok(None) => {}
            }
        }
        finished
    }

    /// Start one handler, capturing both streams to files.
    ///
    /// **Files, not pipes.** A child that writes past the OS pipe buffer
    /// (~64 KiB) blocks until someone reads, and this runner only reads after
    /// `try_wait` reports exit — so a chatty handler would hang until its
    /// timeout killed it, and the output cap guarantees somebody eventually
    /// writes that much. Draining incrementally would need a reader thread per
    /// stream; a file needs neither.
    ///
    /// **No shell, ever.** The command is `argv[0]` and the arguments are the
    /// rest, passed as they are, so a substituted value can never be re-split
    /// or glob-expanded. Standard input is `/dev/null`: a handler must not be
    /// able to read the executor's own stdin, which under `fsm serve` is the
    /// MCP protocol stream.
    pub fn spawn(
        &mut self,
        effect_id: String,
        argv: &[String],
        call: Option<&McpCall>,
    ) -> Result<(), ExecError> {
        let Some((command, arguments)) = argv.split_first() else {
            return Err(spawn_error("", "a handler must name a command"));
        };
        // Two children for one effect could produce two acks over the same
        // derived key with different captured output — the one collision the
        // whole design refuses. Displacing the entry would also orphan the
        // first child, since dropping a `Child` neither kills nor reaps it.
        if self.children.contains_key(&effect_id) {
            return Err(spawn_error(
                command,
                &format!("a run for {effect_id} is already in flight"),
            ));
        }
        let run = SPAWN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stderr_path = self.capture_path(&effect_id, run, "err");
        if let Some(call) = call {
            let running = self.spawn_mcp(command, arguments, &stderr_path, call)?;
            self.children.insert(effect_id, running);
            return Ok(());
        }
        let stdout_path = self.capture_path(&effect_id, run, "out");
        let spawned = create_capture(&stdout_path, command).and_then(|stdout| {
            let stderr = create_capture(&stderr_path, command)?;
            Command::new(command)
                .args(arguments)
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()
                .map_err(|error| spawn_error(command, &error.to_string()))
        });
        let child = match spawned {
            Ok(child) => child,
            Err(error) => {
                // Nothing is running, so nothing will ever collect these.
                let _ = std::fs::remove_file(&stdout_path);
                let _ = std::fs::remove_file(&stderr_path);
                return Err(error);
            }
        };
        self.children.insert(
            effect_id,
            Running::Process {
                child,
                stdout_path,
                stderr_path,
            },
        );
        Ok(())
    }

    /// Start one MCP server and the worker that talks to it.
    ///
    /// **Pipes for stdin and stdout, a file for stderr.** The conversation is
    /// short and strictly alternating, so neither pipe can fill; the server's
    /// logs are unbounded chatter and go to a file for the same reason a
    /// process handler's do — nobody is reading them until the run is over.
    ///
    /// The worker owns the pipes and the runner owns the child, which is what
    /// lets one kill path serve both kinds: killing the child closes the
    /// pipes, which ends the worker's read.
    fn spawn_mcp(
        &self,
        command: &str,
        arguments: &[String],
        stderr_path: &Path,
        call: &McpCall,
    ) -> Result<Running, ExecError> {
        let stderr = create_capture(stderr_path, command)?;
        let spawned = Command::new(command)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_file(stderr_path);
                return Err(spawn_error(command, &error.to_string()));
            }
        };
        // Both are `Some` immediately after a piped spawn; treating their
        // absence as a spawn failure keeps this free of an unwrap that would
        // panic the executor for a subprocess's sake.
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(stderr_path);
            return Err(spawn_error(
                command,
                "the server's pipes could not be taken",
            ));
        };
        let (sender, answers) = channel();
        let tool = call.tool.clone();
        let call_arguments = call.arguments.clone();
        // Detached on purpose. The worker ends when the pipes close, and the
        // runner closes them by killing the child — on a timeout, on a
        // cancellation, and in `Drop`. Joining here would mean waiting for a
        // subprocess inside a tick.
        std::thread::spawn(move || {
            let outcome = converse(stdin, stdout, &tool, &call_arguments);
            // The receiver is gone when the effect was already settled, which
            // is ordinary rather than an error.
            let _ = sender.send(outcome);
        });
        Ok(Running::Mcp {
            child,
            stderr_path: stderr_path.to_path_buf(),
            answer: None,
            answers,
        })
    }

    /// Reap a finished child, non-blocking. The only caller of `try_wait`.
    ///
    /// A child killed by a signal has no exit code; it is reported as
    /// `status: -1` so the pipeline acks it `failed` rather than unwrapping a
    /// `None`.
    pub fn poll(&mut self, effect_id: &str) -> Option<RunOutcome> {
        if matches!(self.children.get(effect_id)?, Running::Mcp { .. }) {
            return self.poll_mcp(effect_id);
        }
        let waited = self.children.get_mut(effect_id)?.child_mut().try_wait();
        let status = match waited {
            Ok(Some(status)) => status.code().unwrap_or(-1),
            Ok(None) => return None,
            Err(_) => {
                // The child cannot be waited on at all. Stop it before letting
                // go of the handle, or the run is both reported failed and
                // left running with nothing able to reach it again.
                if let Some(running) = self.children.get_mut(effect_id) {
                    let _ = running.child_mut().kill();
                    let _ = running.child_mut().wait();
                }
                -1
            }
        };
        let running = self.children.remove(effect_id)?;
        let Running::Process {
            stdout_path,
            stderr_path,
            ..
        } = &running
        else {
            return None;
        };
        Some(RunOutcome::Completed {
            status,
            stdout: take_capture(stdout_path),
            stderr: take_capture(stderr_path),
        })
    }

    /// Collect a finished conversation and stop the server that held it.
    ///
    /// The server is killed rather than waited for. Its work is done the
    /// moment it answers, and a server that keeps running after that would
    /// otherwise hold a concurrency slot for as long as it liked.
    fn poll_mcp(&mut self, effect_id: &str) -> Option<RunOutcome> {
        let running = self.children.get_mut(effect_id)?;
        if !running.collect() {
            return None;
        }
        let mut running = self.children.remove(effect_id)?;
        let _ = running.child_mut().kill();
        let _ = running.child_mut().wait();
        let Running::Mcp {
            answer,
            stderr_path,
            ..
        } = running
        else {
            return None;
        };
        Some(RunOutcome::Mcp {
            outcome: answer?,
            stderr: take_capture(&stderr_path),
        })
    }

    /// Stop an in-flight child and reap it.
    ///
    /// A child that has *already* exited is reported as the completion it was,
    /// not as a kill. The window is real: a deadline is decided from the tick's
    /// `now_ms`, so a handler that finished cleanly a moment before its timeout
    /// is still in the map when the kill is directed, and journaling
    /// `exec/timeout` for it would send the machine down its failure path for a
    /// run that succeeded.
    pub fn kill(&mut self, effect_id: &str, reason: KillReason) -> RunOutcome {
        if let Some(completed) = self.poll(effect_id) {
            return completed;
        }
        if let Some(mut running) = self.children.remove(effect_id) {
            // Killing the child closes an MCP worker's pipes, which is how the
            // worker learns the run is over: there is no second stop signal.
            let _ = running.child_mut().kill();
            let _ = running.child_mut().wait();
            running.discard_captures();
        }
        RunOutcome::Killed { reason }
    }

    fn capture_path(&self, effect_id: &str, run: u64, extension: &str) -> PathBuf {
        let stem: String = effect_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        self.scratch.join(format!("{stem}-{run}.{extension}"))
    }
}

/// Kill and reap every remaining child, then remove the capture directory.
///
/// No *signalled* shutdown runs this. Not `kill -9`, and not Ctrl-C either,
/// because Rust's default handler terminates without unwinding: the children
/// are re-parented and keep running, the capture files stay, and the next
/// executor **cannot adopt them** — it sees the effect still pending and
/// starts a fresh run. That is precisely the at-least-once boundary this plan
/// claims, stated where the code makes it true. There is no pid file and no
/// adoption protocol; a handler whose work already reached the outside world
/// is undone by a compensating effect the machine emits, or not at all.
impl Drop for Runner {
    fn drop(&mut self) {
        for (_, mut running) in std::mem::take(&mut self.children) {
            let _ = running.child_mut().kill();
            let _ = running.child_mut().wait();
        }
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// A directory builder that creates the leaf itself and refuses an existing
/// one, private to this user where the platform can express that.
fn private_directory() -> std::fs::DirBuilder {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
}

/// Enough entropy to make the capture directory's name unguessable in
/// practice, without a random-number generator this workspace does not have.
/// The nanosecond clock is the same source `crash_harness.rs` uses to make its
/// run roots invocation-unique.
fn unique_suffix(attempt: u32) -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0)
        .wrapping_add(u128::from(attempt))
}

fn create_capture(path: &Path, command: &str) -> Result<File, ExecError> {
    File::create(path).map_err(|error| {
        spawn_error(
            command,
            &format!("cannot open the capture file {}: {error}", path.display()),
        )
    })
}

fn take_capture(path: &Path) -> BoundedBytes {
    let captured = BoundedBytes::read_capped(path);
    let _ = std::fs::remove_file(path);
    captured
}

fn spawn_error(argv0: &str, reason: &str) -> ExecError {
    ExecError::new("exec/spawn", format!("cannot run {argv0}: {reason}"))
        .hint("check that the handler's argv[0] exists and is executable")
        .details(Value::Obj(BTreeMap::from([(
            "argv0".into(),
            Value::Str(argv0.into()),
        )])))
}
