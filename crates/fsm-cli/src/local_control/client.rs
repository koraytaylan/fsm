use super::{
    finite_timeout, invalid, owner_uid, private_directory,
    protocol::{self, ControlIdentity, REQUEST_CAP, RESPONSE_CAP},
};
use fsm_core::json::{JsonLimits, Value, parse, write_canonical};
use fsm_execute::service::ShutdownMode;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
    },
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

// Detached filesystem/connect workers remain charged until actually retired.
static CLIENT_WORKERS: AtomicUsize = AtomicUsize::new(0);
const CLIENT_WORKER_CAP: usize = 8;
struct WorkerPermit;
impl WorkerPermit {
    fn acquire() -> io::Result<Self> {
        // Rust 1.89 lacks try_update; retain its identical predecessor at MSRV.
        #[allow(deprecated)]
        let admitted = CLIENT_WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < CLIENT_WORKER_CAP).then_some(count + 1)
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "control client workers occupied; admission and cleanup unconfirmed",
                )
            });
        admitted?;
        Ok(Self)
    }
}
impl Drop for WorkerPermit {
    fn drop(&mut self) {
        CLIENT_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Request stop for the unique endpoint matching a physical data directory.
/// Filesystem discovery, connect and transport are all inside the caller bound.
/// Failure is transport uncertainty and never confirms admission or cleanup.
/// The returned JSON contains the actual control report, not a closure proof.
pub fn stop(
    root: &Path,
    data_dir: &Path,
    mode: ShutdownMode,
    timeout_ms: i64,
) -> io::Result<Value> {
    request(root, data_dir, Request::Stop(mode), timeout_ms)
}

/// Observe the original owner's current bounded metadata without requesting stop.
/// No journal is opened or changed; stale snapshots and transport failure grant
/// no native closure or settlement permission, and existing deadlines stay fixed.
pub fn observe(root: &Path, data_dir: &Path, timeout_ms: i64) -> io::Result<Value> {
    request(root, data_dir, Request::Observe, timeout_ms)
}

#[derive(Clone, Copy)]
enum Request {
    Stop(ShutdownMode),
    Observe,
}

fn request(root: &Path, data_dir: &Path, request: Request, timeout_ms: i64) -> io::Result<Value> {
    let budget = finite_timeout(timeout_ms)?;
    let deadline = Instant::now() + budget;
    let permit = WorkerPermit::acquire()?;
    let root = root.to_path_buf();
    let data_dir = data_dir.to_path_buf();
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("fsm-control-client".into())
        .spawn(move || {
            let _permit = permit;
            let result = exchange(&root, &data_dir, request, timeout_ms, deadline);
            let _ = send.send(result);
        })?;
    receive
        .recv_timeout(remaining(deadline)?)
        .map_err(|_| timed_out())?
}

fn timed_out() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "control transport deadline; admission and cleanup unconfirmed",
    )
}
fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(timed_out)
}

fn exchange(
    root: &Path,
    data_dir: &Path,
    kind: Request,
    timeout_ms: i64,
    deadline: Instant,
) -> io::Result<Value> {
    let (mut stream, identity) = discover(root, data_dir, deadline)?;
    remaining(deadline)?;
    // A connect that completed late must not initiate a late first stop request.
    stream.set_write_timeout(Some(remaining(deadline)?))?;
    let mut request = Vec::new();
    let value = match kind {
        Request::Stop(mode) => protocol::request_value(&identity, mode, timeout_ms),
        Request::Observe => protocol::observation_value(&identity),
    };
    write_canonical(&value, &mut request);
    if request.len() > REQUEST_CAP {
        return Err(invalid("control request exceeds byte limit"));
    }
    request.push(b'\n');
    remaining(deadline)?;
    stream.write_all(&request)?;
    let mut response = Vec::with_capacity(RESPONSE_CAP);
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        if response.len() == RESPONSE_CAP {
            return Err(invalid("control response exceeds byte limit"));
        }
        response.push(byte[0]);
    }
    let value = parse(&response, &JsonLimits::DEFAULT)
        .map_err(|_| invalid("invalid control report JSON"))?;
    let report_kind = match kind {
        Request::Stop(_) => protocol::ReportKind::Terminal,
        Request::Observe => protocol::ReportKind::Observed,
    };
    if !protocol::valid_report(&value, &identity, report_kind) {
        return Err(invalid(
            "control report original identity or schema differs",
        ));
    }
    Ok(value)
}

fn discover(
    root: &Path,
    data_dir: &Path,
    deadline: Instant,
) -> io::Result<(UnixStream, ControlIdentity)> {
    let uid = owner_uid()?;
    private_directory(root, uid)?;
    let physical = fs::metadata(data_dir)?;
    let mut found = None;
    // Directory iteration does not collect unbounded entries or choose a PID.
    for entry in fs::read_dir(root)? {
        remaining(deadline)?;
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.len() != 18 || !name.starts_with("c-") {
            continue;
        }
        let directory = entry.path();
        private_directory(&directory, uid)?;
        let identity_path = directory.join("identity");
        let metadata = fs::symlink_metadata(&identity_path)?;
        if !metadata.is_file()
            || metadata.uid() != uid
            || metadata.mode() & 0o777 != 0o600
            || metadata.len() > REQUEST_CAP as u64
        {
            return Err(invalid(
                "control identity is not a bounded owner-only original file",
            ));
        }
        let mut bytes = Vec::with_capacity(REQUEST_CAP + 1);
        File::open(identity_path)?
            .take((REQUEST_CAP + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > REQUEST_CAP {
            return Err(invalid("control identity exceeds byte limit"));
        }
        let identity = parse(&bytes, &JsonLimits::DEFAULT)
            .ok()
            .and_then(|value| protocol::parse_identity(&value))
            .ok_or_else(|| invalid("invalid control identity schema"))?;
        if name != format!("c-{}", &identity.incarnation[..16]) {
            return Err(invalid("control directory incarnation differs"));
        }
        if identity.store_device != physical.dev() || identity.store_inode != physical.ino() {
            continue;
        }
        let socket = directory.join("s");
        let metadata = fs::symlink_metadata(&socket)?;
        if !metadata.file_type().is_socket()
            || metadata.uid() != uid
            || metadata.mode() & 0o777 != 0o600
        {
            return Err(invalid("control socket is not owner-only and original"));
        }
        // SPEC's local-control discovery: refusal excludes only this listener,
        // never proves native death or permits removal of its original files.
        let stream = match UnixStream::connect(&socket) {
            Ok(stream) => stream,
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => continue,
            Err(error) => return Err(error),
        };
        remaining(deadline)?;
        if found.is_some() {
            return Err(invalid(
                "multiple original executor endpoints; stop target is ambiguous",
            ));
        }
        found = Some((stream, identity));
    }
    found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no matching control endpoint; admission and cleanup unconfirmed",
        )
    })
}
