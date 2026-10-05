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

mod claimed;
mod completion;

pub use claimed::{NativeRun, NativeRunPhase, NativeRunProgress};
pub use completion::NativeCompletion;

const HELPER: &str = "/usr/libexec/fsm-containment-authority";
const RESPONSE_LIMIT: usize = 65540;
const POLL_BUDGET: usize = 65536;

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
    /// The owned helper returned an actual exit status to `try_wait`.
    pub reaped: bool,
    /// The helper output socket returned EOF.
    pub stdout_eof: bool,
    /// The helper diagnostic socket returned EOF.
    pub stderr_eof: bool,
}

/// One owned helper request; claim and closure verification belong to its host.
pub struct NativeRequest {
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

impl NativeRequest {
    /// Start the fixed provisioned helper with one bounded request and deadline.
    pub fn start(
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
        let bytes = canon_bytes(request);
        if bytes.len() > 8192 {
            return Err("native client request exceeds bound".into());
        }
        parse(&bytes, &JsonLimits::DEFAULT)
            .map_err(|_| "native client request exceeds JSON limits")?;
        protected_helper()?;
        let (input, input_peer) = UnixStream::pair().map_err(message)?;
        input.set_nonblocking(true).map_err(message)?;
        input_peer.set_nonblocking(true).map_err(message)?;
        let (stdout, output) = Reader::open(RESPONSE_LIMIT + 1)?;
        let (stderr, diagnostics) = Reader::open(4096)?;
        let mut command = Command::new(HELPER);
        command
            .args(["client-watch", namespace, &generation.to_string()])
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
    pub fn poll(&mut self) -> Result<Option<Value>, String> {
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
    pub fn cancel(&mut self) -> Result<(), String> {
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
    pub fn progress(&self) -> NativeHelperProgress {
        NativeHelperProgress {
            reaped: self.status.is_some(),
            stdout_eof: self.stdout.eof,
            stderr_eof: self.stderr.eof,
        }
    }

    /// Observe actual process reap and both stream EOFs without releasing claims.
    pub fn reap(&mut self) -> Result<bool, String> {
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

impl Drop for NativeRequest {
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
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o7777 != 0o755 {
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
        Some("prepare") if payload == &Value::Null => Ok(()),
        Some("bind") => {
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
        Some("execute" | "close" | "observe") => {
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
            br#"{"action":"prepare","format":"fsm.native-request/1","payload":null}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        assert!(validate_request(&value).is_ok());
    }
}
