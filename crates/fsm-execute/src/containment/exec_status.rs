//! Private pre-grant exec reporting; EOF and metadata never prove closure.

use super::{closed, enrollment, identity, io, number, object, protected_directory, read_value};
use fsm_core::json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{
    DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt, chown,
};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 8] = b"FSMEXEC1";

pub(super) struct Listener {
    directory: PathBuf,
    allocation: u64,
    binding: Value,
    listener: UnixListener,
    challenge: [u8; 32],
    process_input: bool,
}

impl Listener {
    pub(super) fn create(
        directory: &Path,
        allocation: u64,
        binding: &Value,
        kind: &fsm_execute::config::HandlerKind,
    ) -> Result<Self, String> {
        let (claim, _lock) = super::validate_binding(directory, binding, None)?;
        if number(&claim.domain().to_value(), "allocation")? != allocation {
            return Err("exec status allocation differs".into());
        }
        let reserved_identity = object([
            ("device", Value::Num(u64::MAX.to_string())),
            ("inode", Value::Num(u64::MAX.to_string())),
        ]);
        let reserved = material(binding, reserved_identity.clone(), reserved_identity);
        let bytes = fsm_core::canon::canon_bytes(&reserved);
        if bytes.len() as u64 > super::MAX_RECORD {
            return Err("exec status envelope exceeds native byte bound".into());
        }
        fsm_core::json::parse(&bytes, &fsm_core::json::JsonLimits::DEFAULT)
            .map_err(|_| "exec status envelope exceeds native depth bound")?;
        let challenge = challenge()?;
        let base = directory.join(format!("exec-{allocation}"));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&base)
            .map_err(io)?;
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).map_err(io)?;
        let socket = base.join("s");
        let listener = UnixListener::bind(&socket).map_err(io)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).map_err(io)?;
        listener.set_nonblocking(true).map_err(io)?;
        close_on_exec(&listener)?;
        let record = material(
            binding,
            identity(&fs::symlink_metadata(&base).map_err(io)?),
            identity(&fs::symlink_metadata(&socket).map_err(io)?),
        );
        File::open(&base).map_err(io)?.sync_all().map_err(io)?;
        super::publish_once(&record_path(directory, allocation), &record)?;
        Ok(Self {
            directory: directory.into(),
            allocation,
            binding: binding.clone(),
            listener,
            challenge,
            process_input: matches!(kind, fsm_execute::config::HandlerKind::Process),
        })
    }

    pub(super) fn send_challenge(&self, input: &mut UnixStream) -> Result<(), String> {
        let mut bytes = Vec::with_capacity(41);
        bytes.extend(MAGIC);
        bytes.push(u8::from(!self.process_input));
        bytes.extend(self.challenge);
        input.write_all(&bytes).map_err(io)
    }

    pub(super) fn associate(self, domain: &Value, gate: &Value) -> Result<Status, String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let _lock = super::authority_lock(&self.directory)?;
        let handoff = read_value(
            &self
                .directory
                .join(format!("handoff-{}.json", self.allocation)),
            true,
        )?;
        closed(&handoff, &["format", "binding", "gate"])?;
        if super::text(&handoff, "format")? != "fsm.native-launch-handoff/1"
            || handoff.get("binding") != Some(&self.binding)
            || handoff.get("gate") != Some(gate)
            || super::closing::prepared_domain(&self.directory, self.allocation)? != *domain
        {
            return Err("exec status original handoff or domain differs".into());
        }
        let enrolled = enrollment::inspect(domain, deadline)?;
        if enrolled.to_value() != *gate {
            return Err("exec status gate differs from handoff".into());
        }
        sole_gate(domain, enrolled.pid)?;
        let base = self.directory.join(format!("exec-{}", self.allocation));
        let socket = base.join("s");
        matched_paths(&self.directory, self.allocation, &self.binding)?;
        chown(&socket, Some(0), Some(enrolled.group)).map_err(io)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o660)).map_err(io)?;
        chown(&base, Some(0), Some(enrolled.group)).map_err(io)?;
        // Only the nondumpable installed gate receives the inherited-stdin
        // nonce; group access and PID hello alone do not authenticate a peer.
        // Only traversal is opened, after the inaccessible socket is ready.
        fs::set_permissions(&base, fs::Permissions::from_mode(0o710)).map_err(io)?;
        let mut stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(io(error)),
            }
            remaining(deadline)?;
            std::thread::sleep(Duration::from_millis(5));
        };
        stream.set_nonblocking(true).map_err(io)?;
        close_on_exec(&stream)?;
        let mut hello = [0; 44];
        let mut received = 0;
        while received < hello.len() {
            match stream.read(&mut hello[received..]) {
                Ok(0) => return Err("exec status hello ended early".into()),
                Ok(count) => received += count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    remaining(deadline)?;
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(io(error)),
            }
        }
        verify_hello(&hello, enrolled.pid, &self.challenge)?;
        let mut trailing = [0];
        match stream.read(&mut trailing) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(io(error)),
            Ok(_) => {
                return Err("exec status hello ended or has trailing bytes before grant".into());
            }
        }
        if enrollment::inspect(domain, deadline)?.to_value() != *gate {
            return Err("exec status enrollment changed".into());
        }
        sole_gate(domain, enrolled.pid)?;
        // The listener disappears before grant, so user code cannot reopen it.
        drop(self.listener);
        retire(&self.directory, self.allocation)?;
        Ok(Status {
            stream,
            bytes: Vec::with_capacity(13),
            resolved: None,
        })
    }
}

/// Readiness is not a launch secret; each listener obtains a separate nonce.
pub(super) fn ready() -> Result<(), String> {
    challenge().map(|_| ())
}

fn challenge() -> Result<[u8; 32], String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(super::NOFOLLOW_NONBLOCK)
        .open("/dev/random")
        .map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.file_type().is_char_device() || metadata.uid() != 0 || metadata.rdev() != 0x108 {
        return Err("exec status entropy source is not the original kernel random device".into());
    }
    let mut bytes = [0; 32];
    file.read_exact(&mut bytes).map_err(io)?;
    Ok(bytes)
}

fn verify_hello(bytes: &[u8], pid: u32, challenge: &[u8; 32]) -> Result<(), String> {
    if bytes.len() != 44
        || &bytes[..8] != MAGIC
        || u32::from_be_bytes(bytes[8..12].try_into().map_err(|_| "invalid exec hello")?) != pid
        || bytes[12..]
            .iter()
            .zip(challenge)
            .fold(0_u8, |difference, (actual, expected)| {
                difference | (actual ^ expected)
            })
            != 0
    {
        return Err("exec status hello differs from original PID or inherited challenge".into());
    }
    Ok(())
}

fn remaining(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("exec status association deadline".into())
    } else {
        Ok(())
    }
}

fn sole_gate(domain: &Value, pid: u32) -> Result<(), String> {
    let group = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-{}-{}-{}.service",
        super::text(domain, "namespace")?,
        number(domain, "generation")?,
        number(domain, "allocation")?
    ));
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(super::NOFOLLOW_NONBLOCK)
        .open(group.join("cgroup.procs"))
        .map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err("exec status process inventory is not protected".into());
    }
    let mut bytes = Vec::with_capacity(65);
    file.take(65).read_to_end(&mut bytes).map_err(io)?;
    if bytes != format!("{pid}\n").as_bytes()
        || super::closing::prepared_domain(
            &super::authority_path(
                super::text(domain, "namespace")?,
                &number(domain, "generation")?.to_string(),
            )?,
            number(domain, "allocation")?,
        )? != *domain
    {
        return Err("exec status original group does not contain only the gate".into());
    }
    Ok(())
}

fn material(binding: &Value, directory: Value, socket: Value) -> Value {
    object([
        ("format", Value::Str("fsm.native-exec-status/1".into())),
        ("binding", binding.clone()),
        ("directory", directory),
        ("socket", socket),
    ])
}

fn record_path(directory: &Path, allocation: u64) -> PathBuf {
    directory.join(format!("exec-status-{allocation}.json"))
}

fn matched_paths(directory: &Path, allocation: u64, binding: &Value) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(record_path(directory, allocation)).map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o777 != 0o600 {
        return Err("exec status metadata is not private Root material".into());
    }
    let record = read_value(&record_path(directory, allocation), true)?;
    closed(&record, &["format", "binding", "directory", "socket"])?;
    if super::text(&record, "format")? != "fsm.native-exec-status/1"
        || record.get("binding") != Some(binding)
    {
        return Err("exec status original binding differs".into());
    }
    for field in ["directory", "socket"] {
        let value = record.get(field).ok_or("exec status identity missing")?;
        closed(value, &["device", "inode"])?;
        number(value, "device")?;
        if number(value, "inode")? == 0 {
            return Err("exec status identity has zero inode".into());
        }
    }
    let base = directory.join(format!("exec-{allocation}"));
    for (path, field) in [(&base, "directory"), (&base.join("s"), "socket")] {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                let correct_type = if field == "directory" {
                    metadata.is_dir()
                } else {
                    metadata.file_type().is_socket()
                };
                if !correct_type
                    || metadata.uid() != 0
                    || (field == "directory"
                        && ![0o700, 0o710].contains(&(metadata.mode() & 0o777)))
                    || (field == "socket" && ![0o600, 0o660].contains(&(metadata.mode() & 0o777)))
                    || record.get(field) != Some(&identity(&metadata))
                {
                    return Err("exec status original path identity differs".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(record)
}

/// Caller holds authority ownership or has not published entry; no recursive removal.
pub(super) fn retire(directory: &Path, allocation: u64) -> Result<(), String> {
    protected_directory(directory)?;
    match fs::symlink_metadata(directory.join(format!("exec-status-{allocation}.json.pending"))) {
        Ok(_) => return Err("exec status publication is incomplete".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let base = directory.join(format!("exec-{allocation}"));
    match fs::symlink_metadata(record_path(directory, allocation)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match fs::symlink_metadata(&base) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                _ => Err("exec status paths lack original identity metadata".into()),
            };
        }
        Err(error) => return Err(io(error)),
        Ok(_) => {}
    }
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    matched_paths(directory, allocation, &binding)?;
    match fs::remove_file(base.join("s")) {
        Ok(()) => File::open(&base).map_err(io)?.sync_all().map_err(io)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    match fs::remove_dir(base) {
        Ok(()) => File::open(directory).map_err(io)?.sync_all().map_err(io),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(error)),
    }
}

pub(super) fn connect(
    directory: &Path,
    allocation: u64,
    deadline: Instant,
) -> Result<Option<GateStatus>, String> {
    let base = directory.join(format!("exec-{allocation}"));
    match fs::symlink_metadata(&base) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
        Ok(metadata)
            if metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o022 == 0 => {}
        Ok(_) => return Err("exec status route is not Root protected".into()),
    }
    let mut challenge = [0; 41];
    std::io::stdin()
        .lock()
        .read_exact(&mut challenge)
        .map_err(io)?;
    if &challenge[..8] != MAGIC || challenge[8] > 1 {
        return Err("exec status inherited challenge is malformed".into());
    }
    remaining(deadline)?;
    let mut stream = loop {
        super::entry::ensure_open(directory, allocation)?;
        match UnixStream::connect(base.join("s")) {
            Ok(stream) => break stream,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::ConnectionRefused
                ) => {}
            Err(error) => return Err(io(error)),
        }
        remaining(deadline)?;
        std::thread::sleep(Duration::from_millis(5));
    };
    close_on_exec(&stream)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(io)?;
    let mut hello = MAGIC.to_vec();
    hello.extend(std::process::id().to_be_bytes());
    hello.extend_from_slice(&challenge[9..]);
    stream.write_all(&hello).map_err(io)?;
    Ok(Some(GateStatus {
        stream,
        process_input: challenge[8] == 0,
    }))
}

pub(super) struct GateStatus {
    stream: UnixStream,
    process_input: bool,
}

impl GateStatus {
    pub(super) fn restore_input(&self, command: &mut std::process::Command) {
        if self.process_input {
            command.stdin(std::process::Stdio::null());
        }
    }

    pub(super) fn failed(&mut self, error: &std::io::Error) -> Result<(), String> {
        failed(&mut self.stream, error)
    }
}

fn close_on_exec(stream: &impl AsRawFd) -> Result<(), String> {
    if stream.as_raw_fd() < 3 {
        return Err("exec status descriptor overlaps handler stdio".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(super::NOFOLLOW_NONBLOCK)
        .open(format!("/proc/self/fdinfo/{}", stream.as_raw_fd()))
        .map_err(io)?;
    let mut bytes = Vec::with_capacity(4097);
    file.take(4097).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() > 4096 {
        return Err("exec status descriptor inspection exceeds bound".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "invalid exec descriptor inspection")?;
    let flags: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("flags:\t"))
        .collect();
    if flags.len() != 1
        || u32::from_str_radix(flags[0], 8).map_err(|_| "invalid exec descriptor flags")? & 0x80000
            == 0
    {
        return Err("exec status descriptor is not close-on-exec".into());
    }
    Ok(())
}

pub(super) fn failed(stream: &mut UnixStream, error: &std::io::Error) -> Result<(), String> {
    let code = error
        .raw_os_error()
        .filter(|code| *code > 0)
        .ok_or("exec failure lacks an OS error")?;
    let mut frame = MAGIC.to_vec();
    frame.extend(code.to_be_bytes());
    stream.write_all(&frame).map_err(io)
}

pub(super) struct Status {
    stream: UnixStream,
    bytes: Vec<u8>,
    resolved: Option<Option<i32>>,
}

impl Status {
    pub(super) fn refuse_partial(&self) -> Result<(), String> {
        if !self.bytes.is_empty() && self.resolved.is_none() {
            Err("exec status partial frame retains uncertainty".into())
        } else {
            Ok(())
        }
    }

    /// Outer absence means pending; inner absence is EOF without an exec error.
    pub(super) fn poll(&mut self) -> Result<Option<Option<i32>>, String> {
        if let Some(resolved) = self.resolved {
            return Ok(Some(resolved));
        }
        let mut buffer = [0; 13];
        let limit = 13 - self.bytes.len();
        match self.stream.read(&mut buffer[..limit]) {
            Ok(0) => {
                let result = decode(&self.bytes)?;
                self.resolved = Some(result);
                Ok(Some(result))
            }
            Ok(count) => {
                self.bytes.extend_from_slice(&buffer[..count]);
                if self.bytes.len() > 12 {
                    return Err("exec status frame exceeds bound".into());
                }
                Ok(None)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(io(error)),
        }
    }
}

fn decode(bytes: &[u8]) -> Result<Option<i32>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != 12 || &bytes[..8] != MAGIC {
        return Err("exec status frame is partial or malformed".into());
    }
    let code = i32::from_be_bytes(
        bytes[8..]
            .try_into()
            .map_err(|_| "invalid exec status frame")?,
    );
    if code <= 0 {
        return Err("exec status OS error is not positive".into());
    }
    Ok(Some(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_pid_and_inherited_nonce_are_both_required() {
        let challenge = [7; 32];
        let mut hello = MAGIC.to_vec();
        hello.extend(123_u32.to_be_bytes());
        hello.extend(challenge);
        verify_hello(&hello, 123, &challenge).unwrap();
        assert!(verify_hello(&hello, 124, &challenge).is_err());
        for index in 12..44 {
            let mut changed = hello.clone();
            changed[index] ^= 1;
            assert!(verify_hello(&changed, 123, &challenge).is_err());
        }
        assert!(verify_hello(&hello[..43], 123, &challenge).is_err());
        hello.push(0);
        assert!(verify_hello(&hello, 123, &challenge).is_err());
    }

    #[test]
    fn private_exec_status_requires_exact_frame_and_eof() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        close_on_exec(&reader).unwrap();
        close_on_exec(&writer).unwrap();
        reader.set_nonblocking(true).unwrap();
        let mut status = Status {
            stream: reader,
            bytes: Vec::new(),
            resolved: None,
        };
        assert_eq!(status.poll().unwrap(), None);
        failed(&mut writer, &std::io::Error::from_raw_os_error(2)).unwrap();
        assert_eq!(status.poll().unwrap(), None);
        assert_eq!(status.poll().unwrap(), None);
        drop(writer);
        assert_eq!(status.poll().unwrap(), Some(Some(2)));
        assert_eq!(status.poll().unwrap(), Some(Some(2)));
    }

    #[test]
    fn actual_stream_limit_and_partial_frame_refuse_candidate_selection() {
        for count in [1, 11, 12, 13] {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            reader.set_nonblocking(true).unwrap();
            let mut status = Status {
                stream: reader,
                bytes: Vec::with_capacity(13),
                resolved: None,
            };
            let mut frame = MAGIC.to_vec();
            frame.extend(2_i32.to_be_bytes());
            frame.push(0);
            writer.write_all(&frame[..count]).unwrap();
            let sampled = status.poll();
            if count == 13 {
                assert!(sampled.unwrap_err().contains("exceeds bound"));
                assert_eq!(status.bytes.len(), 13);
            } else {
                assert_eq!(sampled.unwrap(), None);
                assert!(status.refuse_partial().is_err());
                drop(writer);
                if count == 12 {
                    assert_eq!(status.poll().unwrap(), Some(Some(2)));
                    status.refuse_partial().unwrap();
                } else {
                    assert!(status.poll().unwrap_err().contains("partial or malformed"));
                }
            }
        }
    }

    #[test]
    fn empty_eof_is_distinct_from_malformed_or_partial_error() {
        assert_eq!(decode(&[]).unwrap(), None);
        for count in 1..12 {
            assert!(decode(&[0; 12][..count]).is_err());
        }
        assert!(decode(&[0; 13]).is_err());
        assert!(decode(&[0; 12]).is_err());
        for code in [0_i32, -1] {
            let mut bytes = MAGIC.to_vec();
            bytes.extend(code.to_be_bytes());
            assert!(decode(&bytes).is_err());
        }
    }
}
