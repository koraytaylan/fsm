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
    validate(&query(
        "system.slice",
        &["LoadState", "ActiveState", "ControlGroup"],
        Instant::now() + Duration::from_secs(2),
    )?)
}

#[cfg(test)]
pub(super) fn properties(unit: &str, keys: &[&str]) -> Result<BTreeMap<String, String>, String> {
    properties_before(unit, keys, Instant::now() + Duration::from_secs(2))
}

pub(super) fn properties_before(
    unit: &str,
    keys: &[&str],
    deadline: Instant,
) -> Result<BTreeMap<String, String>, String> {
    let fields = fields(&query(unit, keys, deadline)?)?;
    if fields.len() != keys.len() || keys.iter().any(|key| !fields.contains_key(*key)) {
        return Err("system-manager property inventory differs".into());
    }
    Ok(fields)
}

fn query(unit: &str, keys: &[&str], requested: Instant) -> Result<Vec<u8>, String> {
    let deadline = requested.min(Instant::now() + Duration::from_secs(2));
    if Instant::now() >= deadline {
        return Err("system-manager query deadline or incomplete I/O".into());
    }
    let binary = binary()?;
    let mut command = Command::new(binary);
    command.args(["show", unit, "--no-pager"]);
    for key in keys {
        command.arg(format!("--property={key}"));
    }
    capture(command, deadline)
}

pub(super) fn stop(unit: &str, deadline: Instant) -> Result<(), String> {
    let mut command = Command::new(binary()?);
    command.args(["stop", "--job-mode=replace", "--no-ask-password", unit]);
    capture(
        command,
        deadline.min(Instant::now() + Duration::from_secs(2)),
    )
    .map(|_| ())
}

fn binary() -> Result<&'static Path, String> {
    let binary = Path::new("/usr/bin/systemctl");
    protected_directory(binary.parent().ok_or("manager binary has no parent")?)?;
    let metadata = fs::symlink_metadata(binary).map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err("system-manager executable is not root protected".into());
    }
    Ok(binary)
}

/// Retirement requires both an unloaded unit and no queued job for its name.
pub(super) fn retired(unit: &str, deadline: Instant) -> Result<bool, String> {
    let mut command = Command::new(binary()?);
    command.args([
        "list-units",
        "--all",
        "--plain",
        "--no-legend",
        "--no-pager",
        unit,
    ]);
    let units = capture(command, deadline)?;
    let units = std::str::from_utf8(&units).map_err(|_| "invalid manager unit inventory")?;
    if !units.trim().is_empty() {
        if units
            .lines()
            .any(|line| line.split_whitespace().next() != Some(unit))
        {
            return Err("manager unit inventory differs".into());
        }
        return Ok(false);
    }
    let mut command = Command::new(binary()?);
    command.args(["list-jobs", "--plain", "--no-legend", "--no-pager"]);
    jobs_clear(&capture(command, deadline)?, unit)
}

fn jobs_clear(bytes: &[u8], unit: &str) -> Result<bool, String> {
    let output = std::str::from_utf8(bytes).map_err(|_| "invalid manager job inventory")?;
    if output.trim().is_empty() || output.trim() == "No jobs running." {
        return Ok(true);
    }
    let mut clear = true;
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 4
            || fields[0]
                .parse::<u64>()
                .ok()
                .filter(|id| *id > 0)
                .is_none_or(|id| fields[0] != id.to_string())
        {
            return Err("manager job inventory differs".into());
        }
        if fields[1] == unit {
            clear = false;
        }
    }
    Ok(clear)
}

fn capture(mut command: Command, deadline: Instant) -> Result<Vec<u8>, String> {
    if Instant::now() >= deadline {
        return Err("system-manager query deadline or incomplete I/O".into());
    }
    let (mut stdout, output) = UnixStream::pair().map_err(io)?;
    let (mut stderr, diagnostics) = UnixStream::pair().map_err(io)?;
    stdout.set_nonblocking(true).map_err(io)?;
    stderr.set_nonblocking(true).map_err(io)?;
    let output: OwnedFd = output.into();
    let diagnostics: OwnedFd = diagnostics.into();
    let mut query = Query(
        command
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::from(output))
            .stderr(Stdio::from(diagnostics))
            .spawn()
            .map_err(io)?,
    );
    // Command owns the original descriptors even after spawn: keeping it
    // alive would keep both streams open after the manager exits.
    drop(command);
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
                return Ok(output);
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

fn fields(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let output = std::str::from_utf8(bytes).map_err(|_| "invalid manager response encoding")?;
    let mut fields = BTreeMap::new();
    for line in output.lines() {
        let (key, value) = line.split_once('=').ok_or("invalid manager property")?;
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err("duplicate manager property".into());
        }
    }
    Ok(fields)
}

fn validate(bytes: &[u8]) -> Result<(), String> {
    if fields(bytes)?
        != BTreeMap::from([
            ("LoadState".into(), "loaded".into()),
            ("ActiveState".into(), "active".into()),
            ("ControlGroup".into(), "/system.slice".into()),
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
    fn expired_manager_observation_refuses_before_spawn() {
        assert!(
            properties_before("system.slice", &["ActiveState"], Instant::now())
                .unwrap_err()
                .contains("query deadline")
        );
    }

    #[test]
    fn manager_job_inventory_refuses_matching_jobs_and_malformed_rows() {
        assert!(jobs_clear(b"", "owned.service").unwrap());
        assert!(jobs_clear(b"No jobs running.\n", "owned.service").unwrap());
        assert!(jobs_clear(b"7 other.service start waiting\n", "owned.service").unwrap());
        assert!(!jobs_clear(b"7 owned.service stop running\n", "owned.service").unwrap());
        for bytes in [
            b"0 other.service start waiting\n".as_slice(),
            b"07 other.service start waiting\n",
            b"7 other.service start\n",
            b"7 other.service start waiting extra\n",
            b"\xff",
        ] {
            assert!(jobs_clear(bytes, "owned.service").is_err());
        }
    }

    #[test]
    fn production_capture_observes_eof_after_child_exit() {
        let bytes = capture(
            Command::new("/usr/bin/true"),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn manager_capture_bounds_exact_limit_and_refuses_excess() {
        for count in [4096, 4097] {
            let (mut reader, mut writer) = UnixStream::pair().unwrap();
            reader.set_nonblocking(true).unwrap();
            writer.write_all(&vec![b'x'; count]).unwrap();
            drop(writer);
            let mut retained = Vec::with_capacity(4096);
            let mut eof = false;
            let deadline = Instant::now() + Duration::from_secs(2);
            let result = loop {
                let result = drain(&mut reader, &mut retained, &mut eof);
                if result.is_err() || eof {
                    break result;
                }
                assert!(Instant::now() < deadline, "manager capture EOF timed out");
                std::thread::sleep(Duration::from_millis(1));
            };
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
        writer.write_all(b"x").unwrap();
        drain(&mut reader, &mut retained, &mut eof).unwrap();
        assert_eq!(retained.as_slice(), b"x");
        assert!(!eof);
        drop(writer);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !eof {
            drain(&mut reader, &mut retained, &mut eof).unwrap();
            assert!(
                eof || Instant::now() < deadline,
                "manager peer EOF timed out"
            );
            if !eof {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(eof);
        assert_eq!(retained.as_slice(), b"x");
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
