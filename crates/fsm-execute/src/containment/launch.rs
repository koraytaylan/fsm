//! Durable single-submission manager launch; never a closure receipt.

use super::{
    catalogue, enrollment, io, number, object, protected_directory, publish_once, read_value, text,
    validate_binding,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub(super) fn begin(
    directory: &Path,
    allocation: u64,
    streams: [Stdio; 3],
) -> Result<(Child, Duration), String> {
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    let (claim, _lock) = validate_binding(directory, &binding, None)?;
    let domain = claim.domain().to_value();
    if number(&domain, "allocation")? != allocation {
        return Err("launch allocation differs from binding".into());
    }
    let intent_path = directory.join(format!("launch-{allocation}.json"));
    match fs::symlink_metadata(&intent_path) {
        Ok(_) => return Err("native launch already submitted or uncertain".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    enrollment::installed()?;
    let executable = Path::new("/usr/bin/systemd-run");
    protected_directory(executable.parent().ok_or("launcher has no parent")?)?;
    let metadata = fs::symlink_metadata(executable).map_err(io)?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o111 == 0
    {
        return Err("system-manager launcher is not root protected".into());
    }
    let catalogue = catalogue::read(directory)?;
    let claim_value = claim.to_value();
    let fingerprint = text(&claim_value, "handler_fingerprint")?;
    let handler = catalogue
        .handlers
        .values()
        .find(|handler| handler.fingerprint() == fingerprint)
        .ok_or("launch approved handler missing")?;
    let runtime_ms = u64::try_from(handler.timeout_ms)
        .map_err(|_| "invalid approved timeout")?
        .checked_add(5000)
        .ok_or("launch deadline exceeds bound")?;
    let intent = object([
        ("format", Value::Str("fsm.native-launch-intent/1".into())),
        ("binding", binding),
    ]);
    // Reserve the extra container before any durable submission marker.
    parse(&canon_bytes(&intent), &JsonLimits::DEFAULT)
        .map_err(|_| "launch intent exceeds native depth bound")?;
    let namespace = text(&domain, "namespace")?;
    let generation = number(&domain, "generation")?.to_string();
    let allocation_text = allocation.to_string();
    let unit = format!("fsm-containment-{namespace}-{generation}-{allocation}.service");
    let mut command = Command::new(executable);
    command
        .args(["--quiet", "--collect", "--pipe", "--service-type=exec"])
        .arg(format!("--unit={unit}"));
    for property in [
        "DynamicUser=yes",
        "ProtectControlGroups=yes",
        "ProtectHome=yes",
        "ProtectProc=invisible",
        "RestrictNamespaces=yes",
        "NoNewPrivileges=yes",
        "CapabilityBoundingSet=",
        "Delegate=no",
        "ExitType=cgroup",
        "KillMode=control-group",
        "KillSignal=SIGKILL",
        "Restart=no",
        "TimeoutStopSec=2s",
    ] {
        command.arg(format!("--property={property}"));
    }
    let [stdin, stdout, stderr] = streams;
    command
        .arg(format!("--property=RuntimeMaxSec={runtime_ms}ms"))
        .args([
            enrollment::EXECUTABLE,
            "gate",
            namespace,
            &generation,
            &allocation_text,
        ])
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr);
    publish_once(&intent_path, &intent)?;
    let child = command.spawn().map_err(io)?;
    Ok((child, Duration::from_millis(runtime_ms)))
}

struct Monitor<'a> {
    directory: &'a Path,
    allocation: u64,
    child: Child,
}

impl Drop for Monitor<'_> {
    fn drop(&mut self) {
        let _ = super::termination::request(self.directory, self.allocation);
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

pub(super) fn run(directory: &Path, allocation: u64) -> Result<(), String> {
    let (child, bound) = begin(
        directory,
        allocation,
        [Stdio::inherit(), Stdio::inherit(), Stdio::inherit()],
    )?;
    let mut monitor = Monitor {
        directory,
        allocation,
        child,
    };
    let deadline = Instant::now() + bound + Duration::from_secs(3);
    loop {
        if let Some(status) = monitor.child.try_wait().map_err(io)? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("gate transport exited: {status}"))
            };
        }
        if Instant::now() >= deadline {
            return Err("gate transport deadline exceeded; native ownership retained".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
