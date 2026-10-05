//! One bounded canonical request/response frame with a shared I/O deadline.

use super::{closed, io, text};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub(super) fn read(stream: &mut UnixStream) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut header = [0; 4];
    exact(stream, &mut header, deadline)?;
    let length = u32::from_be_bytes(header) as usize;
    if !(1..=8192).contains(&length) {
        return Err("broker frame exceeds request bound".into());
    }
    let mut bytes = vec![0; length];
    exact(stream, &mut bytes, deadline)?;
    let value = parse(&bytes, &JsonLimits::DEFAULT).map_err(|_| "broker frame JSON invalid")?;
    if canon_bytes(&value) != bytes {
        return Err("broker frame is not canonical".into());
    }
    validate(&value)?;
    Ok(value)
}

fn exact(stream: &mut UnixStream, mut bytes: &mut [u8], deadline: Instant) -> Result<(), String> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("broker frame deadline")?;
        if remaining.is_zero() {
            return Err("broker frame deadline".into());
        }
        stream.set_read_timeout(Some(remaining)).map_err(io)?;
        match stream.read(bytes) {
            Ok(0) => return Err("broker frame incomplete".into()),
            Ok(count) => bytes = &mut bytes[count..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(())
}

pub(super) fn validate(value: &Value) -> Result<(), String> {
    closed(value, &["format", "action", "payload"])?;
    if text(value, "format")? != "fsm.native-request/1" {
        return Err("broker request format differs".into());
    }
    let payload = value.get("payload").ok_or("broker payload missing")?;
    match text(value, "action")? {
        "prepare" if payload == &Value::Null => Ok(()),
        "bind" => closed(payload, &["format", "claim", "journal_claim"]),
        "execute" | "close" | "observe" => allocation(payload).map(|_| ()),
        _ => Err("broker action or payload outside policy".into()),
    }
}

pub(super) fn allocation(value: &Value) -> Result<u64, String> {
    let raw = value.as_num().ok_or("broker allocation is not a number")?;
    let number = raw
        .parse::<u64>()
        .map_err(|_| "broker allocation invalid")?;
    if number == 0 || raw != number.to_string() {
        return Err("broker allocation noncanonical".into());
    }
    Ok(number)
}

pub(super) fn write(stream: &mut UnixStream, value: &Value) -> Result<(), String> {
    let bytes = canon_bytes(value);
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err("broker response exceeds bound".into());
    }
    let mut framed = Vec::with_capacity(bytes.len() + 4);
    framed.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    framed.extend_from_slice(&bytes);
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut pending = framed.as_slice();
    while !pending.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("broker response deadline")?;
        if remaining.is_zero() {
            return Err("broker response deadline".into());
        }
        stream.set_write_timeout(Some(remaining)).map_err(io)?;
        match stream.write(pending) {
            Ok(0) => return Err("broker response incomplete".into()),
            Ok(count) => pending = &pending[count..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::object;
    use super::*;

    #[test]
    fn closed_policy_refuses_paths_commands_aliases_and_unbounded_frames() {
        let request = object([
            ("format", Value::Str("fsm.native-request/1".into())),
            ("action", Value::Str("prepare".into())),
            ("payload", Value::Null),
        ]);
        assert!(validate(&request).is_ok());
        for raw in ["0", "01", "-1", "1.0", "18446744073709551616"] {
            assert!(allocation(&Value::Num(raw.into())).is_err());
        }
        assert_eq!(allocation(&Value::Num("1".into())).unwrap(), 1);
        let (mut receiver, mut sender) = UnixStream::pair().unwrap();
        sender.write_all(&8193u32.to_be_bytes()).unwrap();
        assert!(read(&mut receiver).unwrap_err().contains("bound"));
        let (mut receiver, mut sender) = UnixStream::pair().unwrap();
        let bytes = canon_bytes(&request);
        sender
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .unwrap();
        sender.write_all(&bytes).unwrap();
        assert_eq!(read(&mut receiver).unwrap(), request);
        let mut fields = request.as_obj().unwrap().clone();
        fields.insert("path".into(), Value::Str("/tmp/other".into()));
        assert!(validate(&Value::Obj(fields)).is_err());
        let mut fields = request.as_obj().unwrap().clone();
        fields.insert("action".into(), Value::Str("authorize".into()));
        assert!(validate(&Value::Obj(fields)).is_err());
    }
}
