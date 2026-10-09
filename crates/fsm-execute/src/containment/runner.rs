//! Claimed process/MCP execution through one enrolled native cleanup path.

use super::{
    authorize, catalogue, closure, io, launch, number, object, read_value, stop, text,
    validate_binding,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_execute::config::{HandlerKind, substitute, substitute_arguments};
use fsm_execute::mcp_client::McpOutcome;
use fsm_execute::run::native_io::{NativeCapture, NativeProtocol};
use fsm_execute::run::{KillReason, RunOutcome};
use fsm_store::store::{Store, VerifiedClosure};
#[cfg(test)]
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[path = "completion_record.rs"]
mod completion_record;
#[path = "process_exit.rs"]
mod process_exit;

pub(super) fn recover(directory: &Path, allocation: u64) -> Result<Value, String> {
    completion_record::recover(directory, allocation)
}

// The native fixture supplies actual retired-run material to the production
// publisher; this adapter adds no publication or verification behavior.
#[cfg(test)]
pub(super) fn fixture_publish_completion(
    directory: &Path,
    claim: &fsm_core::record::execution::Claim,
    result: &Value,
) -> Result<(), String> {
    completion_record::publish(directory, claim, result)
}

struct OwnedRun {
    directory: PathBuf,
    allocation: u64,
    child: Child,
    worker: Option<NativeProtocol>,
    native_closed: bool,
}

impl Drop for OwnedRun {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        if !self.native_closed {
            let _ = stop::fence(&self.directory, self.allocation);
        }
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
        if let Some(worker) = &mut self.worker {
            worker.reap();
        }
    }
}

enum Candidate {
    Process(i32),
    Spawn,
    Mcp(McpOutcome),
    Timeout,
    Cancelled,
}

pub(super) fn execute(directory: &Path, allocation: u64) -> Result<Value, String> {
    execute_cancellable(directory, allocation, &AtomicBool::new(false))
}

pub(super) fn execute_cancellable(
    directory: &Path,
    allocation: u64,
    cancelled: &AtomicBool,
) -> Result<Value, String> {
    // Keep original publication serialized even after native closure is visible.
    let _execution_lease = super::runner_lease::acquire(directory, allocation)?;
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    let (claim, lock) = validate_binding(directory, &binding, None)
        .map_err(|error| format!("runner binding validation refused: {error}"))?;
    if number(&claim.domain().to_value(), "allocation")? != allocation {
        return Err("runner allocation differs from claim".into());
    }
    let registration = read_value(&directory.join("store.json"), true)?;
    let store = Store::open_read_only(Path::new(text(&registration, "path")?))
        .map_err(|error| error.message)?;
    let pending =
        fsm_execute::effect::resolve(&store, claim.effect().1).map_err(|error| error.message)?;
    let table = catalogue::read(directory)?;
    let handler = table
        .handlers
        .get(&pending.effect_name)
        .ok_or("runner approved handler missing")?;
    let argv = substitute(&handler.argv, &pending.args).map_err(|error| error.message)?;
    catalogue::verify(directory, &store, &claim, Some(&argv))?;
    let kind = match &handler.kind {
        HandlerKind::Process => HandlerKind::Process,
        HandlerKind::Mcp { tool, arguments } => HandlerKind::Mcp {
            tool: tool.clone(),
            arguments: substitute_arguments(arguments, &pending.args)
                .map_err(|error| error.message)?,
        },
    };
    let timeout = Duration::from_millis(
        u64::try_from(handler.timeout_ms).map_err(|_| "runner timeout invalid")?,
    );
    drop(store);
    drop(lock);
    let (mut stderr, error_output) = NativeCapture::open().map_err(|error| error.message)?;
    let mut stdout = None;
    let mut protocol = None;
    let (mut input, input_peer) = UnixStream::pair().map_err(io)?;
    let streams = match &kind {
        HandlerKind::Process => {
            let (capture, output) = NativeCapture::open().map_err(|error| error.message)?;
            stdout = Some(capture);
            [Stdio::from(OwnedFd::from(input_peer)), output, error_output]
        }
        HandlerKind::Mcp { .. } => {
            let (output, output_peer) = UnixStream::pair().map_err(io)?;
            protocol = Some(output);
            [
                Stdio::from(OwnedFd::from(input_peer)),
                Stdio::from(OwnedFd::from(output_peer)),
                error_output,
            ]
        }
    };
    if cancelled.load(Ordering::Acquire) {
        return Err("runner cancelled before launch; claim remains unresolved".into());
    }
    let exec_listener =
        super::exec_status::Listener::create(directory, allocation, &binding, &kind)?;
    exec_listener.send_challenge(&mut input)?;
    let (child, _) = launch::begin(directory, allocation, streams)
        .map_err(|error| format!("runner launch refused: {error}"))?;
    let mut owned = OwnedRun {
        directory: directory.into(),
        allocation,
        child,
        worker: None,
        native_closed: false,
    };
    let argv0 = argv[0].clone();
    let grant = object([
        ("format", Value::Str("fsm.native-entry/1".into())),
        ("claim", claim.to_value()),
        (
            "journal_claim",
            binding
                .get("journal_claim")
                .ok_or("runner claim hash missing")?
                .clone(),
        ),
        (
            "argv",
            Value::Arr(argv.into_iter().map(Value::Str).collect()),
        ),
    ]);
    let handoff = read_value(&directory.join(format!("handoff-{allocation}.json")), true)
        .map_err(|error| format!("runner cleanup uncertain: handoff read failed: {error}"))?;
    let gate = handoff
        .get("gate")
        .ok_or("runner cleanup uncertain: protected gate missing")?;
    let mut exec_status = exec_listener.associate(&claim.domain().to_value(), gate)?;
    if let HandlerKind::Mcp { tool, arguments } = kind {
        let output = protocol.ok_or("runner protocol streams missing")?;
        owned.worker =
            Some(NativeProtocol::from_streams(input, output, tool, arguments).map_err(io)?);
    } else {
        drop(input);
    }

    #[cfg(test)]
    hold_native_crash_cut(directory, &claim, None, cancelled, "authorization")?;
    if !cancelled.load(Ordering::Acquire) {
        authorize::publish_enrolled(directory, &object([("grant", grant)]))
            .map_err(|error| format!("runner enrolled entry refused: {error}"))?;
    }
    let deadline = Instant::now() + timeout;
    let observation_interval = (timeout / 4).min(Duration::from_millis(100));
    let mut root_observation = Instant::now() + observation_interval;
    let candidate = loop {
        let exec_observation = exec_status.poll()?;
        if exec_observation.is_some_and(|error| error.is_some()) {
            break Candidate::Spawn;
        }
        if cancelled.load(Ordering::Acquire) {
            exec_status.refuse_partial()?;
            break Candidate::Cancelled;
        }
        stderr.poll();
        if let Some(stdout) = &mut stdout {
            stdout.poll();
        }
        if exec_observation == Some(None)
            && let Some(worker) = &mut owned.worker
            && let Some(answer) = worker.collect()
        {
            break Candidate::Mcp(answer);
        }
        // The owned child is systemd-run, not the invocation-matched handler;
        // its retirement cannot authenticate the handler's exit status.
        owned.child.try_wait().map_err(io)?;
        if process_exit::handler_deadline_expired(deadline, Instant::now()) {
            exec_status.refuse_partial()?;
            break Candidate::Timeout;
        }
        if exec_observation == Some(None)
            && owned.worker.is_none()
            && Instant::now() >= root_observation
        {
            match process_exit::observe(&claim.domain().to_value(), gate, deadline) {
                Ok(Some(status)) => break Candidate::Process(status),
                Ok(None) => {}
                Err(error) => {
                    if process_exit::deadline_expired(&error, deadline, Instant::now()) {
                        break Candidate::Timeout;
                    }
                    return Err(format!("runner root inspection uncertain: {error}"));
                }
            }
            root_observation = Instant::now() + observation_interval;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    #[cfg(test)]
    hold_native_crash_cut(directory, &claim, Some(&candidate), cancelled, "candidate")?;
    if let Some(worker) = &owned.worker {
        worker.cancel();
    }
    // A naturally retired unit may no longer support the live stop operation;
    // only independent complete-close proof can resolve that uncertainty.
    let stop_result = stop::fence(directory, allocation);
    closure::complete(directory, allocation).map_err(|error| match stop_result {
        Ok(()) => format!("runner cleanup uncertain: {error}; matched stop completed"),
        Err(stop_error) => {
            format!("runner cleanup uncertain: {error}; matched stop refused: {stop_error}")
        }
    })?;
    owned.native_closed = true;
    let receipt = directory.join(format!("closure-{allocation}-{}.json", claim.run_id()));
    VerifiedClosure::read(&receipt).map_err(|error| error.message)?;
    let retirement = Instant::now() + Duration::from_secs(1);
    loop {
        stderr.poll();
        if let Some(stdout) = &mut stdout {
            stdout.poll();
        }
        let child_reaped = owned.child.try_wait().map_err(io)?.is_some();
        let worker_reaped = owned.worker.as_mut().is_none_or(NativeProtocol::reap);
        if child_reaped && worker_reaped {
            break;
        }
        if Instant::now() >= retirement {
            return Err("runner cleanup uncertain: owned handles remain".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    #[cfg(test)]
    hold_native_crash_cut(
        directory,
        &claim,
        Some(&candidate),
        cancelled,
        "domain-closed",
    )?;
    let stderr = stderr.finish();
    let outcome = match candidate {
        Candidate::Spawn => RunOutcome::SpawnFailed { argv0 },
        Candidate::Process(status) => RunOutcome::Completed {
            status,
            stdout: stdout.ok_or("runner process capture missing")?.finish(),
            stderr,
        },
        Candidate::Mcp(outcome) => RunOutcome::Mcp { outcome, stderr },
        Candidate::Timeout => RunOutcome::Killed {
            reason: KillReason::Timeout,
        },
        Candidate::Cancelled => RunOutcome::Killed {
            reason: KillReason::Cancelled,
        },
    };
    let result = object([
        ("format", Value::Str("fsm.native-run-result/3".into())),
        ("handler_contract", handler.contract_value()),
        (
            "handler_kind",
            Value::Str(
                match &handler.kind {
                    HandlerKind::Process => "process",
                    HandlerKind::Mcp { .. } => "mcp",
                }
                .into(),
            ),
        ),
        ("claim", claim.to_value()),
        (
            "journal_claim",
            binding
                .get("journal_claim")
                .ok_or("runner claim hash missing")?
                .clone(),
        ),
        (
            "receipt",
            Value::Str(
                receipt
                    .to_str()
                    .ok_or("runner receipt path invalid")?
                    .into(),
            ),
        ),
        ("candidate", outcome.ack_result()),
        (
            "failure_class",
            outcome
                .failure_class()
                .map_or(Value::Null, |class| Value::Str(class.into())),
        ),
    ]);
    if canon_bytes(&result).len() > 65536 {
        return Err("runner result exceeds response bound".into());
    }
    completion_record::publish(directory, &claim, &result)?;
    Ok(result)
}

#[cfg(test)]
fn hold_native_crash_cut(
    directory: &Path,
    claim: &fsm_core::record::execution::Claim,
    candidate: Option<&Candidate>,
    cancelled: &AtomicBool,
    cut: &str,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let request = directory.join("crash-candidate-barrier.json");
    if !request.try_exists().map_err(io)? {
        return Ok(());
    }
    let request = read_value(&request, true)?;
    let requested_cut = match request.get("cut") {
        Some(value) => value.as_str().ok_or("invalid test crash cut")?,
        None => "candidate",
    };
    if requested_cut != cut {
        return Ok(());
    }
    if number(&request, "attempt")? != number(&claim.to_value(), "attempt")? {
        return Ok(());
    }
    if request.get("run_id").is_some() && number(&request, "run_id")? != claim.run_id() {
        return Ok(());
    }
    let kind = candidate.map(|candidate| match candidate {
        Candidate::Process(_) => "process",
        Candidate::Mcp(_) => "mcp",
        Candidate::Spawn => "spawn",
        Candidate::Timeout => "timeout",
        Candidate::Cancelled => "cancelled",
    });
    let ready = directory.join(format!("crash-{cut}-{}.json", claim.run_id()));
    let mut observation = BTreeMap::from([
        ("claim".into(), claim.to_value()),
        ("cut".into(), Value::Str(cut.into())),
    ]);
    if let Some(kind) = kind {
        observation.insert("candidate".into(), Value::Str(kind.into()));
    }
    if cut == "authorization" {
        let allocation = number(&claim.domain().to_value(), "allocation")?;
        observation.insert(
            "handoff".into(),
            read_value(&directory.join(format!("handoff-{allocation}.json")), true)?,
        );
        if directory
            .join(format!("entry-{allocation}.json"))
            .try_exists()
            .map_err(io)?
        {
            return Err("test authorization cut already published its grant".into());
        }
    }
    super::publish_once(&ready, &Value::Obj(observation))?;
    std::fs::set_permissions(&ready, std::fs::Permissions::from_mode(0o444)).map_err(io)?;
    // Only a protected test request enables this observation; production builds
    // contain no barrier; the observer can retain the original descendants
    // across immediate restart before releasing the ordinary closure path.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cancelled.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            return Err("test candidate barrier expired without executor death".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if let Some(release) = request.get("closure_release") {
        let release = Path::new(release.as_str().ok_or("invalid test closure release")?);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !release.try_exists().map_err(io)? {
            if Instant::now() >= deadline {
                return Err("test candidate barrier expired before closure release".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    Ok(())
}
