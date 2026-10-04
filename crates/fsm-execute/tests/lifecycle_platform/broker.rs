//! Private native privilege-protocol prototype, not a shipped broker.
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt, chown};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::{Duration, Instant};

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
    if !matches!(action, "allocate" | "inspect" | "launch" | "close") {
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
    let socket = base.join("control.sock");
    let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    chown(&socket, Some(uid), None).map_err(|e| e.to_string())?;
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
        let result = request(&mut stream).and_then(|operation| {
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
        let response = match result {
            Ok(value) => format!("ok\n{value}"),
            Err(error) => format!("error\n{error}\n"),
        };
        // A client that vanished cannot stop the broker or release ownership.
        let _ = stream.write_all(response.as_bytes());
    }
    fs::remove_file(socket).map_err(|e| e.to_string())
}
