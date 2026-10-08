//! Supervised native broker transport; a response is not closure proof.

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use std::fs;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(crate) mod worker;

mod claimed;
mod completion;
mod discovery;
mod execution;
mod preparation;
mod prepared_cleanup;
mod prepared_owner;
mod proof_worker;
mod shutdown;
mod startup;
#[cfg(test)]
mod test_support;
mod unwind;

pub use claimed::{NativeRun, NativeRunPhase, NativeRunProgress};
pub use completion::NativeCompletion;
pub use execution::{NativeExecution, NativeExecutionProgress};
pub use preparation::{NativePreparation, NativePreparationPhase, NativePreparationProgress};
pub use prepared_cleanup::NativePreparedCleanup;
pub use prepared_owner::NativePreparedOwner;
pub use shutdown::NativeShutdown;

/// Wrap an embedding panic hook to allow only internally marked native workers to unwind.
///
/// This installs no process hook; pass the returned closure to `set_hook`.
/// Unmarked panics reach the original hook unchanged. Worker join failure
/// retains uncertainty and grants no native closure or journal ownership.
pub fn filter_native_worker_panics<F>(
    hook: F,
) -> impl Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static
where
    F: Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static,
{
    move |info| {
        if !unwind::is_isolated() {
            hook(info);
        }
    }
}

const HELPER: &str = "/usr/libexec/fsm-containment-authority";
const RESPONSE_LIMIT: usize = 65540;
const POLL_BUDGET: usize = 65536;

fn refusal(response: &Value, prefix: &str) -> Option<String> {
    let fields = response.as_obj()?;
    if fields.len() != 3
        || response.get("format").and_then(Value::as_str) != Some("fsm.native-response/1")
        || response.get("ok") != Some(&Value::Bool(false))
    {
        return None;
    }
    response
        .get("result")
        .and_then(Value::as_str)
        .map(|reason| super::native_admission::bounded_diagnostic(prefix, reason))
}

pub(super) fn discover_store(store: &Path) -> Result<(String, u64), String> {
    discovery::discover(store)
}

pub(super) fn check_claim_store(
    store: &Path,
    claim: &fsm_core::record::execution::Claim,
) -> Result<(), String> {
    discovery::check_claim(store, claim)
}

pub(crate) fn completion_published(
    store: &Path,
    claim: &fsm_core::record::execution::Claim,
) -> Result<bool, String> {
    discovery::completion_published(store, claim)
}

struct Reader {
    stream: UnixStream,
    bytes: Vec<u8>,
    limit: usize,
    eof: bool,
}

impl Reader {
    fn open(limit: usize) -> Result<(Self, Stdio), String> {
        let (stream, writer) = UnixStream::pair().map_err(message)?;
        stream.set_nonblocking(true).map_err(message)?;
        Ok((
            Self {
                stream,
                bytes: Vec::with_capacity(limit),
                limit,
                eof: false,
            },
            Stdio::from(OwnedFd::from(writer)),
        ))
    }

    fn drain(&mut self) -> Result<(), String> {
        let mut budget = POLL_BUDGET;
        let mut buffer = [0; 8192];
        while !self.eof && budget > 0 {
            let offered = buffer.len().min(budget);
            match self.stream.read(&mut buffer[..offered]) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(count) => {
                    let kept = count.min(self.limit.saturating_sub(self.bytes.len()));
                    self.bytes.extend_from_slice(&buffer[..kept]);
                    budget -= count;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(message(error)),
            }
        }
        Ok(())
    }
}

/// Observed transport retirement facts; none authenticate handler-tree closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeHelperProgress {
    /// A joined worker finished startup without creating a helper; no closure is implied.
    pub not_started: bool,
    /// The owned helper returned an actual exit status to `try_wait`.
    pub reaped: bool,
    /// The helper output socket returned EOF.
    pub stdout_eof: bool,
    /// The helper diagnostic socket returned EOF.
    pub stderr_eof: bool,
}

impl NativeHelperProgress {
    /// Whether the observed transport is empty or actually reaped with both EOFs.
    /// This does not establish native domain closure or release a durable claim.
    pub fn is_retired(self) -> bool {
        self.not_started || (self.reaped && self.stdout_eof && self.stderr_eof)
    }
}

/// One owned helper request; claim and closure verification belong to its host.
struct InlineRequest {
    child: Child,
    input: Option<UnixStream>,
    pending: Vec<u8>,
    written: usize,
    stdout: Reader,
    stderr: Reader,
    status: Option<ExitStatus>,
    deadline: Instant,
    error: Option<String>,
    collected: bool,
}

/// One original transport, with explicitly selected owned worker polling.
pub struct NativeRequest {
    inline: Option<InlineRequest>,
    worker: Option<worker::Worker>,
    ticket: Option<std::sync::Arc<worker::Ticket>>,
}

impl NativeRequest {
    /// Dispatch fixed-helper startup to the selected worker, or start synchronously standalone.
    pub fn start(
        namespace: &str,
        generation: u64,
        request: &Value,
        timeout: Duration,
    ) -> Result<Self, String> {
        let prepared = PreparedRequest::prepare(namespace, generation, request, timeout)?;
        let ticket = worker::reserve_current()?;
        Self::start_reserved(prepared, ticket)
    }

    fn successor(
        &self,
        namespace: &str,
        generation: u64,
        request: &Value,
        deadline: Instant,
    ) -> Result<Self, String> {
        if !self.progress().is_retired() {
            return Err(
                "native original transport has not retired before successor startup".into(),
            );
        }
        let prepared = PreparedRequest::prepare_until(namespace, generation, request, deadline)?;
        // Sequential phases of one original attempt share its reservation;
        // the retired predecessor cannot run concurrently with this helper.
        let ticket = match &self.ticket {
            Some(ticket) => Some(std::sync::Arc::clone(ticket)),
            None => worker::reserve_current()?,
        };
        Self::start_reserved(prepared, ticket)
    }

    fn start_until(
        namespace: &str,
        generation: u64,
        request: &Value,
        deadline: Instant,
    ) -> Result<Self, String> {
        let prepared = PreparedRequest::prepare_until(namespace, generation, request, deadline)?;
        Self::start_reserved(prepared, worker::reserve_current()?)
    }

    fn start_reserved(
        prepared: PreparedRequest,
        ticket: Option<std::sync::Arc<worker::Ticket>>,
    ) -> Result<Self, String> {
        let startup = startup::Startup::new(prepared);
        if let Some(ticket) = ticket {
            let worker = worker::Worker::start_prepared(startup, std::sync::Arc::clone(&ticket))?;
            return Ok(Self {
                inline: None,
                worker: Some(worker),
                ticket: Some(ticket),
            });
        }
        let inline = startup.start()?;
        Ok(Self {
            inline: Some(inline),
            worker: None,
            ticket: None,
        })
    }

    fn ensure_worker(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Ok(());
        }
        if self.ticket.is_none() {
            self.ticket = worker::reserve_current()?;
        }
        let Some(ticket) = self.ticket.as_ref() else {
            return Ok(());
        };
        let inline = self
            .inline
            .take()
            .expect("original inline transport retained");
        match worker::Worker::start(inline, std::sync::Arc::clone(ticket)) {
            Ok(worker) => {
                self.worker = Some(worker);
                Ok(())
            }
            Err(inline) => {
                self.inline = Some(*inline);
                Err("native transport worker unavailable; original helper retained".into())
            }
        }
    }

    /// Poll the original transport; worker mode never performs helper I/O here.
    pub fn poll(&mut self) -> Result<Option<Value>, String> {
        self.ensure_worker()?;
        match self.worker.as_mut() {
            Some(worker) => worker.poll(),
            None => self.inline.as_mut().expect("original transport").poll(),
        }
    }

    /// Request original helper cancellation without granting domain closure.
    pub fn cancel(&mut self) -> Result<(), String> {
        self.ensure_worker()?;
        match self.worker.as_mut() {
            Some(worker) => {
                worker.cancel();
                Ok(())
            }
            None => self.inline.as_mut().expect("original transport").cancel(),
        }
    }

    /// Read actual cached helper observations, with worker retirement explicit.
    pub fn progress(&self) -> NativeHelperProgress {
        match self.worker.as_ref() {
            Some(worker) => worker.progress(),
            None => self.inline.as_ref().expect("original transport").progress(),
        }
    }

    /// Observe transport retirement; worker mode joins only a finished worker.
    /// Joined startup refusal retires an empty transport without child reap or EOF.
    pub fn reap(&mut self) -> Result<bool, String> {
        self.ensure_worker()?;
        match self.worker.as_mut() {
            Some(worker) => Ok(worker.reap()),
            None => self.inline.as_mut().expect("original transport").reap(),
        }
    }
}

/// Immutable bounded startup material; no store, sockets or child ownership.
struct PreparedRequest {
    namespace: String,
    generation: u64,
    bytes: Vec<u8>,
    deadline: Instant,
}

impl PreparedRequest {
    fn prepare_until(
        namespace: &str,
        generation: u64,
        request: &Value,
        deadline: Instant,
    ) -> Result<Self, String> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("native run deadline; claim remains uncertain")?;
        let mut prepared = Self::prepare(namespace, generation, request, remaining)?;
        // Validation and serialization consume the original attempt's time;
        // deriving another Instant from the remaining duration would extend it.
        prepared.deadline = deadline;
        Ok(prepared)
    }

    fn prepare(
        namespace: &str,
        generation: u64,
        request: &Value,
        timeout: Duration,
    ) -> Result<Self, String> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
            return Err("native client platform unsupported".into());
        }
        if namespace.len() != 32
            || !namespace
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || generation == 0
            || timeout.is_zero()
        {
            return Err("native client route or deadline invalid".into());
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("native client deadline exceeds clock range")?;
        validate_request(request)?;
        let bytes = crate::value_limits::canonical(request, 8192)
            .map_err(|_| "native client request exceeds bound")?;
        parse(&bytes, &JsonLimits::DEFAULT)
            .map_err(|_| "native client request exceeds JSON limits")?;
        Ok(Self {
            namespace: namespace.into(),
            generation,
            bytes,
            deadline,
        })
    }
}

impl InlineRequest {
    fn start_prepared(prepared: PreparedRequest) -> Result<Self, String> {
        let PreparedRequest {
            namespace,
            generation,
            bytes,
            deadline,
        } = prepared;
        protected_helper()?;
        let (input, input_peer) = UnixStream::pair().map_err(message)?;
        input.set_nonblocking(true).map_err(message)?;
        input_peer.set_nonblocking(true).map_err(message)?;
        let (stdout, output) = Reader::open(RESPONSE_LIMIT + 1)?;
        let (stderr, diagnostics) = Reader::open(4096)?;
        let mut command = Command::new(HELPER);
        command
            .args(["client-watch", &namespace, &generation.to_string()])
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .stdin(Stdio::from(OwnedFd::from(input_peer)))
            .stdout(output)
            .stderr(diagnostics);
        let child = command.spawn().map_err(message)?;
        // Command retains original endpoints after spawn; release those copies
        // so EOF can authenticate actual helper stream retirement.
        drop(command);
        let mut pending = Vec::with_capacity(bytes.len() + 4);
        pending.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        pending.extend_from_slice(&bytes);
        Ok(Self {
            child,
            input: Some(input),
            pending,
            written: 0,
            stdout,
            stderr,
            status: None,
            deadline,
            error: None,
            collected: false,
        })
    }

    /// Poll bounded I/O; return a response only after successful reap and EOF.
    fn poll(&mut self) -> Result<Option<Value>, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.collected {
            return Err("native client response already collected".into());
        }
        if Instant::now() >= self.deadline {
            return self.fail("native client deadline; claim remains uncertain".into());
        }
        if let Some(input) = &mut self.input
            && self.written < self.pending.len()
        {
            match input.write(&self.pending[self.written..]) {
                Ok(0) => return self.fail("native client request write incomplete".into()),
                Ok(count) => {
                    self.written += count;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return self.fail(message(error)),
            }
        }
        let retired = match self.reap() {
            Ok(retired) => retired,
            Err(error) => return self.fail(error),
        };
        if self.stdout.bytes.len() > RESPONSE_LIMIT {
            return self.fail("native client response exceeds bound".into());
        }
        if Instant::now() >= self.deadline {
            return self.fail("native client deadline; claim remains uncertain".into());
        }
        if retired {
            if !self.status.as_ref().is_some_and(ExitStatus::success) {
                return self.fail(format!(
                    "native client helper failed: {}",
                    String::from_utf8_lossy(&self.stderr.bytes)
                ));
            }
            let response = match decode_response(&self.stdout.bytes) {
                Ok(response) => response,
                Err(error) => return self.fail(error),
            };
            self.collected = true;
            return Ok(Some(response));
        }
        Ok(None)
    }

    /// Request helper death; this never proves handler or native domain closure.
    fn cancel(&mut self) -> Result<(), String> {
        self.error
            .get_or_insert_with(|| "native client cancelled; claim remains uncertain".into());
        self.input.take();
        if self.status.is_none() {
            self.status = self.child.try_wait().map_err(message)?;
        }
        if self.status.is_none() {
            self.child.kill().map_err(message)?;
        }
        Ok(())
    }

    /// Read the last observed cleanup facts without polling, I/O or claim release.
    fn progress(&self) -> NativeHelperProgress {
        NativeHelperProgress {
            not_started: false,
            reaped: self.status.is_some(),
            stdout_eof: self.stdout.eof,
            stderr_eof: self.stderr.eof,
        }
    }

    /// Observe actual process reap and both stream EOFs without releasing claims.
    fn reap(&mut self) -> Result<bool, String> {
        let stdout = self.stdout.drain();
        let stderr = self.stderr.drain();
        let process = if self.status.is_none() {
            self.child
                .try_wait()
                .map(|status| self.status = status)
                .map_err(message)
        } else {
            Ok(())
        };
        if self.status.is_some() {
            self.input.take();
        }
        stdout?;
        stderr?;
        process?;
        Ok(self.status.is_some() && self.stdout.eof && self.stderr.eof)
    }

    fn fail(&mut self, error: String) -> Result<Option<Value>, String> {
        self.error = Some(error.clone());
        let _ = self.cancel();
        Err(error)
    }
}

impl Drop for InlineRequest {
    fn drop(&mut self) {
        let _ = self.cancel();
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.status.is_none() && Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(status) => self.status = status,
                Err(_) => break,
            }
        }
    }
}

fn message(error: std::io::Error) -> String {
    error.to_string()
}

fn protected_helper() -> Result<(), String> {
    let path = Path::new(HELPER);
    for ancestor in path
        .parent()
        .ok_or("native helper parent missing")?
        .ancestors()
    {
        let metadata = fs::symlink_metadata(ancestor).map_err(message)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err("native helper ancestor is not root protected".into());
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(message)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o7777 != 0o711 {
        return Err("native helper is not protected ordinary executable".into());
    }
    Ok(())
}

fn validate_request(value: &Value) -> Result<(), String> {
    let fields = value.as_obj().ok_or("native request is not an object")?;
    if fields.len() != 3
        || value.get("format").and_then(Value::as_str) != Some("fsm.native-request/1")
        || !fields.contains_key("payload")
    {
        return Err("native request shape differs".into());
    }
    let payload = value
        .get("payload")
        .ok_or("native request payload missing")?;
    match value.get("action").and_then(Value::as_str) {
        Some("prepare-owned") if payload == &Value::Null => Ok(()),
        Some("discard-prepared") => fsm_core::record::execution::NativeDomain::from_value(payload)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Some("bind" | "close-claimed" | "reconcile-claimed") => {
            let binding = payload.as_obj().ok_or("native binding is not an object")?;
            if binding.len() == 3
                && ["format", "claim", "journal_claim"]
                    .iter()
                    .all(|field| binding.contains_key(*field))
            {
                Ok(())
            } else {
                Err("native binding shape differs".into())
            }
        }
        Some("execute" | "close" | "observe" | "recover") => {
            let raw = payload
                .as_num()
                .ok_or("native allocation is not a number")?;
            let allocation = raw
                .parse::<u64>()
                .map_err(|_| "native allocation invalid")?;
            if allocation != 0 && raw == allocation.to_string() {
                Ok(())
            } else {
                Err("native allocation noncanonical".into())
            }
        }
        _ => Err("native request outside broker policy".into()),
    }
}

fn decode_response(framed: &[u8]) -> Result<Value, String> {
    if framed.len() < 4 || framed.len() > RESPONSE_LIMIT {
        return Err("native response framing incomplete".into());
    }
    let length = u32::from_be_bytes(
        framed[..4]
            .try_into()
            .map_err(|_| "native response header missing")?,
    ) as usize;
    if length == 0 || length != framed.len() - 4 {
        return Err("native response body length differs".into());
    }
    let body = &framed[4..];
    let value = parse(body, &JsonLimits::DEFAULT).map_err(|_| "native response JSON invalid")?;
    let fields = value.as_obj().ok_or("native response is not an object")?;
    if canon_bytes(&value) != body
        || fields.len() != 3
        || value.get("format").and_then(Value::as_str) != Some("fsm.native-response/1")
        || !matches!(value.get("ok"), Some(Value::Bool(_)))
        || !fields.contains_key("result")
    {
        return Err("native response shape or canonical form differs".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    #[test]
    fn refusal_requires_closed_envelope_and_bounds_sanitized_reason() {
        use fsm_core::json::Value;
        use std::collections::BTreeMap;
        let mut fields = BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-response/1".into())),
            ("ok".into(), Value::Bool(false)),
            ("result".into(), Value::Str("authority\nbusy".into())),
        ]);
        assert_eq!(
            super::refusal(&Value::Obj(fields.clone()), "refused: ").unwrap(),
            "refused: authority busy"
        );
        fields.insert("result".into(), Value::Str("é".repeat(1024)));
        assert!(
            super::refusal(&Value::Obj(fields.clone()), "refused: ")
                .unwrap()
                .len()
                <= 1024
        );
        fields.insert("extra".into(), Value::Null);
        assert!(super::refusal(&Value::Obj(fields.clone()), "refused: ").is_none());
        fields.remove("extra");
        fields.insert("ok".into(), Value::Bool(true));
        assert!(super::refusal(&Value::Obj(fields), "refused: ").is_none());
    }
    use super::*;

    fn framed(body: &[u8]) -> Vec<u8> {
        let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn responses_require_exact_framing_canonical_shape_and_no_extra_bytes() {
        let body = br#"{"format":"fsm.native-response/1","ok":true,"result":null}"#;
        let valid = framed(body);
        assert_eq!(
            decode_response(&valid).unwrap().get("ok"),
            Some(&Value::Bool(true))
        );
        let mut extra = valid.clone();
        extra.push(0);
        assert!(decode_response(&extra).is_err());
        assert!(decode_response(&valid[..valid.len() - 1]).is_err());
        assert!(decode_response(&framed(b"null")).is_err());
        assert!(
            decode_response(&framed(
                br#"{"format":"fsm.native-response/1","ok":1,"result":null}"#
            ))
            .is_err()
        );
        assert!(
            decode_response(&framed(
                br#"{ "format":"fsm.native-response/1","ok":true,"result":null}"#
            ))
            .is_err()
        );
        assert!(decode_response(&vec![0; RESPONSE_LIMIT + 1]).is_err());
    }

    #[test]
    fn reconciliation_request_reaches_transport_only_with_closed_claim_binding_shape() {
        use std::collections::BTreeMap;
        let binding = BTreeMap::from([
            (
                "format".into(),
                Value::Str("fsm.native-claim-binding/1".into()),
            ),
            ("claim".into(), Value::Null),
            (
                "journal_claim".into(),
                Value::Str(format!("sha256:{}", "a".repeat(64))),
            ),
        ]);
        let mut request = BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-request/1".into())),
            ("action".into(), Value::Str("reconcile-claimed".into())),
            ("payload".into(), Value::Obj(binding.clone())),
        ]);
        // Shape permission only; the authority independently parses and matches
        // the actual original claim before closure can have any effect.
        assert!(validate_request(&Value::Obj(request.clone())).is_ok());
        let mut changed = binding.clone();
        changed.insert("path".into(), Value::Str("arbitrary".into()));
        request.insert("payload".into(), Value::Obj(changed));
        assert!(validate_request(&Value::Obj(request.clone())).is_err());
        request.insert("payload".into(), Value::Obj(binding));
        request.insert("action".into(), Value::Str("force-reconcile".into()));
        assert!(validate_request(&Value::Obj(request)).is_err());
    }

    #[test]
    fn recovery_is_an_allocation_only_request_without_launch_aliases() {
        use std::collections::BTreeMap;
        let mut fields = BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-request/1".into())),
            ("action".into(), Value::Str("recover".into())),
            ("payload".into(), Value::Num("1".into())),
        ]);
        assert!(validate_request(&Value::Obj(fields.clone())).is_ok());
        for payload in [
            Value::Num("0".into()),
            Value::Num("01".into()),
            Value::Str("/tmp/completed.json".into()),
            Value::Null,
        ] {
            fields.insert("payload".into(), payload);
            assert!(validate_request(&Value::Obj(fields.clone())).is_err());
        }
        fields.insert("payload".into(), Value::Num("1".into()));
        fields.insert("action".into(), Value::Str("recover-and-execute".into()));
        assert!(validate_request(&Value::Obj(fields)).is_err());
    }

    #[test]
    fn retained_writer_is_not_eof_and_diagnostic_prefix_stays_bounded() {
        let (stream, mut peer) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut reader = Reader {
            stream,
            bytes: Vec::new(),
            limit: 4096,
            eof: false,
        };
        peer.write_all(&[7; 8192]).unwrap();
        reader.drain().unwrap();
        assert_eq!(reader.bytes, [7; 4096]);
        assert!(!reader.eof);
        reader.drain().unwrap();
        assert!(!reader.eof);
        drop(peer);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !reader.eof {
            reader.drain().unwrap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(reader.bytes, [7; 4096]);
    }

    #[test]
    fn host_request_policy_refuses_unbounded_capability_and_allocation_aliases() {
        for raw in ["0", "01", "-1", "1.0", "18446744073709551616"] {
            let request = Value::Obj(std::collections::BTreeMap::from([
                ("format".into(), Value::Str("fsm.native-request/1".into())),
                ("action".into(), Value::Str("execute".into())),
                ("payload".into(), Value::Num(raw.into())),
            ]));
            assert!(validate_request(&request).is_err());
        }
        let value = parse(
            br#"{"action":"authorize","format":"fsm.native-request/1","payload":null}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        assert!(validate_request(&value).is_err());
        let value = parse(
            br#"{"action":"prepare-owned","format":"fsm.native-request/1","payload":null}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        assert!(validate_request(&value).is_ok());
    }

    #[test]
    fn host_request_policy_accepts_owned_preparation_only_with_null_payload() {
        for (payload, accepted) in [("null", true), ("0", false), ("{}", false)] {
            let request = format!(
                "{{\"action\":\"prepare-owned\",\"format\":\"fsm.native-request/1\",\"payload\":{payload}}}"
            );
            let value = parse(request.as_bytes(), &JsonLimits::DEFAULT).unwrap();
            assert_eq!(validate_request(&value).is_ok(), accepted);
        }
    }

    #[test]
    fn host_request_policy_refuses_legacy_preparation_before_transport() {
        let value = parse(
            br#"{"action":"prepare","format":"fsm.native-request/1","payload":null}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        assert!(validate_request(&value).is_err());
        match NativeRequest::start(
            "0123456789abcdef0123456789abcdef",
            1,
            &value,
            Duration::from_secs(3),
        ) {
            Err(error) => assert_eq!(error, "native request outside broker policy"),
            Ok(_) => panic!("legacy preparation reached native transport startup"),
        }
    }
}
