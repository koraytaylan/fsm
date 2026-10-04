//! Private native privilege-protocol prototype, not a shipped broker.
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "endpoint.rs"]
mod endpoint;

const MAX_FRAME: usize = 256;

fn request(stream: &mut UnixStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now();
    let mut bytes = Vec::new();
    loop {
        if deadline.elapsed() >= Duration::from_millis(300) {
            return Err("request deadline".into());
        }
        let mut byte = [0];
        if stream.read(&mut byte).map_err(|e| e.to_string())? != 1 {
            return Err("incomplete frame".into());
        }
        if byte[0] == b'\n' {
            break;
        }
        if bytes.len() == MAX_FRAME {
            return Err("frame limit".into());
        }
        bytes.push(byte[0]);
    }
    let request = String::from_utf8(bytes).map_err(|_| "invalid frame encoding")?;
    let operation = request
        .strip_prefix("privilege/1 ")
        .ok_or("invalid protocol")?;
    let action = operation.split(':').next().ok_or("missing action")?;
    if !matches!(
        action,
        "allocate" | "inspect" | "launch" | "close" | "lease"
    ) {
        return Err("operation outside fixed privilege policy".into());
    }
    Ok(operation.to_owned())
}

pub(super) fn serve(base: &Path, uid: u32) -> Result<(), String> {
    let root = fs::symlink_metadata(base).map_err(|e| e.to_string())?;
    let data = fs::symlink_metadata(base.join("data")).map_err(|e| e.to_string())?;
    if fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid() != 0
        || uid == 0
        || !root.is_dir()
        || root.uid() != 0
        || root.mode() & 0o022 != 0
        || !data.is_dir()
        || data.uid() != 0
        || data.mode() & 0o077 != 0
    {
        return Err("broker needs protected provisioned authority and operator UID".into());
    }
    // chmod after bind cannot revoke a connection already queued during a
    // permissive creation window. Require the process mask to exclude group
    // and other writers before publishing even the initial root-owned socket.
    let mut status = String::new();
    fs::File::open("/proc/self/status")
        .map_err(|e| e.to_string())?
        .take(4097)
        .read_to_string(&mut status)
        .map_err(|e| e.to_string())?;
    let mask = status
        .lines()
        .find_map(|line| line.strip_prefix("Umask:"))
        .and_then(|value| u32::from_str_radix(value.trim(), 8).ok());
    if status.len() > 4096 || mask.is_none_or(|value| value & 0o022 != 0o022) {
        return Err("broker requires a restrictive socket creation mask".into());
    }
    let (_authority, listener, socket) = endpoint::bind(base, uid)?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    fs::write(base.join("broker-ready"), b"ready").map_err(|e| e.to_string())?;
    let lifetime = Instant::now();
    while lifetime.elapsed() < Duration::from_secs(20) {
        let (mut stream, _) = match listener.accept() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        stream
            .set_write_timeout(Some(Duration::from_millis(250)))
            .map_err(|e| e.to_string())?;
        let operation = request(&mut stream);
        let lease = operation
            .as_ref()
            .ok()
            .and_then(|value| value.strip_prefix("lease:"))
            .map(str::to_owned);
        let result = operation.and_then(|operation| {
            let operation = lease
                .as_ref()
                .map_or(operation, |handle| format!("launch:{handle}"));
            super::identity_root::run(base, &operation)?;
            let mut bytes = Vec::new();
            let file = fs::File::open(base.join("response")).map_err(|e| e.to_string())?;
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 4096 {
                return Err("reply limit".into());
            }
            String::from_utf8(bytes).map_err(|e| e.to_string())
        });
        let launched = result.is_ok();
        let response = match result {
            Ok(value) => format!("ok\n{value}"),
            Err(error) => format!("error\n{error}\n"),
        };
        // A client that vanished cannot stop the broker or release ownership.
        let delivered = stream.write_all(response.as_bytes()).is_ok();
        if let Some(handle) = lease.filter(|_| launched) {
            let watchdog = Instant::now();
            let mut notification = if delivered {
                "watchdog"
            } else {
                "delivery-failed"
            };
            while delivered && watchdog.elapsed() < Duration::from_secs(5) {
                let mut byte = [0];
                match stream.read(&mut byte) {
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Ok(0) => {
                        notification = "eof";
                        break;
                    }
                    Ok(_) => {
                        notification = "client-request";
                        break;
                    }
                    Err(_) => {
                        notification = "connection-error";
                        break;
                    }
                }
            }
            // Diagnostic only, never an ownership or closure authority.
            fs::write(base.join("lease-notification"), notification).map_err(|e| e.to_string())?;
            // Failure retains protected ownership and fails this prototype;
            // disconnect never substitutes for actual domain closure.
            super::identity_root::run(base, &format!("close:{handle}"))?;
        }
    }
    fs::remove_file(socket).map_err(|e| e.to_string())
}

/// Native operator fixture. A descendant remains after main-process death,
/// proving its exec did not retain the socket used for death notification.
pub(super) fn client(base: &Path, handle: &str) -> Result<(), String> {
    if handle.len() > 128 {
        return Err("client handle limit".into());
    }
    let mut stream =
        UnixStream::connect(endpoint::connect_path(base)?).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(format!("privilege/1 lease:{handle}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    while response.iter().filter(|byte| **byte == b'\n').count() < 6 {
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        if response.len() == 512 {
            return Err("client reply limit".into());
        }
        response.push(byte[0]);
    }
    if !response.starts_with(b"ok\nidentity/1\n") {
        return Err("lease launch refused".into());
    }
    let work = base.join("work");
    let mut child = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["native_fixture", "--exact", "--nocapture"])
        .env("FSM_LIFECYCLE_PROBE_DIRECTORY", &work)
        .env("FSM_LIFECYCLE_PROBE_MODE", "leaf-lease")
        .spawn()
        .map_err(|e| e.to_string())?;
    fs::write(work.join("lease-client-ready"), b"lease held").map_err(|e| e.to_string())?;
    // The test kills the main process while the child remains in a separate
    // client domain. Its fixture watchdog bounds emergency cleanup.
    child.wait().map_err(|e| e.to_string())?;
    Ok(())
}
