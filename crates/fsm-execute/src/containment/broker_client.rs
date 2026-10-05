//! Unprivileged transport helper; the host owns and bounds this process.

use super::{
    authority_path, broker_frame, closed, identity, io, number, protected_directory, read_value,
    text,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

fn route(directory: &Path) -> Result<(Value, PathBuf), String> {
    protected_directory(directory)?;
    let base = directory.join("broker");
    protected_directory(&base)?;
    let route_path = base.join("route.json");
    let metadata = fs::symlink_metadata(&route_path).map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o777 != 0o444 {
        return Err("client route is not immutable root publication".into());
    }
    let value = read_value(&route_path, true)?;
    closed(&value, &["format", "configuration", "epoch", "socket"])?;
    let configuration = value
        .get("configuration")
        .ok_or("client configuration missing")?;
    closed(configuration, &["format", "authority", "operator", "boot"])?;
    let uid = number(configuration, "operator")?;
    let epoch = number(&value, "epoch")?;
    if text(&value, "format")? != "fsm.native-broker-route/1"
        || text(configuration, "format")? != "fsm.native-broker-config/1"
        || uid == 0
        || uid >= u64::from(u32::MAX)
        || (61184..=65519).contains(&uid)
        || uid != u64::from(fs::metadata("/proc/self").map_err(io)?.uid())
        || epoch == 0
        || epoch > 4096
        || configuration.get("authority")
            != Some(&identity(&fs::symlink_metadata(directory).map_err(io)?))
        || text(configuration, "boot")?
            != fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .map_err(io)?
                .trim()
    {
        return Err("client operator, boot or authority differs".into());
    }
    let socket = base.join(format!("s-{epoch}"));
    let metadata = fs::symlink_metadata(&socket).map_err(io)?;
    if !metadata.file_type().is_socket()
        || u64::from(metadata.uid()) != uid
        || metadata.mode() & 0o777 != 0o600
        || value.get("socket") != Some(&identity(&metadata))
    {
        return Err("client socket identity or access differs".into());
    }
    Ok((value, socket))
}

fn read_frame(input: &mut impl Read, limit: usize) -> Result<Value, String> {
    let mut header = [0; 4];
    input.read_exact(&mut header).map_err(io)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > limit {
        return Err("client frame exceeds bound".into());
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes).map_err(io)?;
    let value = parse(&bytes, &JsonLimits::DEFAULT).map_err(|_| "client frame JSON invalid")?;
    if canon_bytes(&value) != bytes {
        return Err("client frame is not canonical".into());
    }
    Ok(value)
}

pub(super) fn run(arguments: &[OsString]) -> Result<(), String> {
    if arguments.len() != 2 {
        return Err("client requires exactly namespace and generation".into());
    }
    let directory = authority_path(
        arguments[0].to_str().ok_or("client namespace invalid")?,
        arguments[1].to_str().ok_or("client generation invalid")?,
    )?;
    let request = read_frame(&mut std::io::stdin().lock(), 8192)?;
    broker_frame::validate(&request)?;
    let (original, socket) = route(&directory)?;
    // Connect and blocking reads remain in this supervised, killable process;
    // no background connection thread or late request survives host abort.
    let mut stream = UnixStream::connect(&socket).map_err(io)?;
    if route(&directory)? != (original.clone(), socket.clone()) {
        return Err("client route changed before dispatch".into());
    }
    broker_frame::write(&mut stream, &request)?;
    let response = read_frame(&mut stream, 65536)?;
    closed(&response, &["format", "ok", "result"])?;
    if text(&response, "format")? != "fsm.native-response/1"
        || !matches!(response.get("ok"), Some(Value::Bool(_)))
    {
        return Err("client response shape differs".into());
    }
    if route(&directory)? != (original, socket) {
        return Err("client route changed after response".into());
    }
    let bytes = canon_bytes(&response);
    let mut output = std::io::stdout().lock();
    output
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .map_err(io)?;
    output.write_all(&bytes).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn supervised_helper_refuses_bad_lengths_partial_and_noncanonical_frames() {
        for header in [0u32, 8193, u32::MAX] {
            assert!(read_frame(&mut Cursor::new(header.to_be_bytes()), 8192).is_err());
        }
        let mut partial = 5u32.to_be_bytes().to_vec();
        partial.extend_from_slice(b"nul");
        assert!(read_frame(&mut Cursor::new(partial), 8192).is_err());
        let mut noncanonical = 5u32.to_be_bytes().to_vec();
        noncanonical.extend_from_slice(b"null ");
        assert!(read_frame(&mut Cursor::new(noncanonical), 8192).is_err());
        let mut canonical = 4u32.to_be_bytes().to_vec();
        canonical.extend_from_slice(b"null");
        assert_eq!(
            read_frame(&mut Cursor::new(canonical), 8192).unwrap(),
            Value::Null
        );
    }
}
