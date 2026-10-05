//! Bounded read-only system-manager capability verification before allocation.

use super::{io, protected_directory};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const LIMIT: usize = 4096;

struct Query(Child);

impl Drop for Query {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

pub(super) fn require() -> Result<(), String> {
    let binary = Path::new("/usr/bin/systemctl");
    protected_directory(binary.parent().ok_or("manager binary has no parent")?)?;
    let metadata = fs::symlink_metadata(binary).map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err("system-manager executable is not root protected".into());
    }
    let (mut stdout, output) = UnixStream::pair().map_err(io)?;
    let (mut stderr, diagnostics) = UnixStream::pair().map_err(io)?;
    stdout.set_nonblocking(true).map_err(io)?;
    stderr.set_nonblocking(true).map_err(io)?;
    let output: OwnedFd = output.into();
    let diagnostics: OwnedFd = diagnostics.into();
    let mut query = Query(
        Command::new(binary)
            .args([
                "show",
                "system.slice",
                "--no-pager",
                "--property=LoadState",
                "--property=ActiveState",
                "--property=ControlGroup",
            ])
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::from(output))
            .stderr(Stdio::from(diagnostics))
            .spawn()
            .map_err(io)?,
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut output = Vec::with_capacity(LIMIT);
    let mut diagnostics = Vec::with_capacity(LIMIT);
    let mut output_eof = false;
    let mut diagnostics_eof = false;
    loop {
        drain(&mut stdout, &mut output, &mut output_eof)?;
        drain(&mut stderr, &mut diagnostics, &mut diagnostics_eof)?;
        if let Some(status) = query.0.try_wait().map_err(io)? {
            if !status.success() {
                return Err("system-manager query failed".into());
            }
            if output_eof && diagnostics_eof {
                return validate(&output);
            }
        }
        if Instant::now() >= deadline {
            return Err("system-manager query deadline or incomplete I/O".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn drain(stream: &mut UnixStream, retained: &mut Vec<u8>, eof: &mut bool) -> Result<(), String> {
    if *eof {
        return Ok(());
    }
    let mut chunk = [0; 512];
    for _ in 0..16 {
        match stream.read(&mut chunk) {
            Ok(0) => {
                *eof = true;
                return Ok(());
            }
            Ok(count) => {
                if count > LIMIT.saturating_sub(retained.len()) {
                    return Err("system-manager query output exceeds bound".into());
                }
                retained.extend_from_slice(&chunk[..count]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(())
}

fn validate(bytes: &[u8]) -> Result<(), String> {
    let output = std::str::from_utf8(bytes).map_err(|_| "invalid manager response encoding")?;
    let mut fields = BTreeMap::new();
    for line in output.lines() {
        let (key, value) = line.split_once('=').ok_or("invalid manager property")?;
        if fields.insert(key, value).is_some() {
            return Err("duplicate manager property".into());
        }
    }
    if fields
        != BTreeMap::from([
            ("LoadState", "loaded"),
            ("ActiveState", "active"),
            ("ControlGroup", "/system.slice"),
        ])
    {
        return Err("system-manager slice is unavailable or differs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn manager_capture_bounds_exact_limit_and_refuses_excess() {
        for count in [4096, 4097] {
            let (mut reader, mut writer) = UnixStream::pair().unwrap();
            reader.set_nonblocking(true).unwrap();
            writer.write_all(&vec![b'x'; count]).unwrap();
            drop(writer);
            let mut retained = Vec::with_capacity(4096);
            let mut eof = false;
            let result = drain(&mut reader, &mut retained, &mut eof);
            assert_eq!(retained.len(), 4096);
            assert_eq!(retained.capacity(), 4096);
            assert_eq!(result.is_ok(), count == 4096);
            assert_eq!(eof, count == 4096);
        }
    }

    #[test]
    fn retained_manager_writer_never_turns_would_block_into_eof() {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let mut retained = Vec::with_capacity(4096);
        let mut eof = false;
        drain(&mut reader, &mut retained, &mut eof).unwrap();
        assert!(retained.is_empty());
        assert!(!eof);
        writer.write_all(b'x').unwrap();
        drain(&mut reader, &mut retained, &mut eof).unwrap();
        assert_eq!(retained.as_slice(), b"x");
        assert!(!eof);
        drop(writer);
        drain(&mut reader, &mut retained, &mut eof).unwrap();
        assert!(eof);
    }

    #[test]
    fn manager_properties_require_exact_active_system_slice() {
        assert!(
            validate(b"LoadState=loaded\nActiveState=active\nControlGroup=/system.slice\n").is_ok()
        );
        for bytes in [
            b"LoadState=loaded\n".as_slice(),
            b"LoadState=not-found\nActiveState=inactive\nControlGroup=\n".as_slice(),
            b"LoadState=loaded\nActiveState=active\nControlGroup=/user.slice\n",
            b"LoadState=loaded\nActiveState=active\nControlGroup=/system.slice\nLoadState=loaded\n",
            b"LoadState=loaded\nActiveState=active\nControlGroup=/system.slice\nUnknown=x\n",
            b"invalid",
            b"\xff",
        ] {
            assert!(validate(bytes).is_err());
        }
    }
}
