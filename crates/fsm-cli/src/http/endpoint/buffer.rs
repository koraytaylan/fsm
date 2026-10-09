//! Aggregate buffered response allocation, independent of queued frame charges.
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

pub(super) const RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    failed: bool,
    sealed: bool,
}
impl State {
    fn refuse(&mut self) {
        self.failed = true;
        self.bytes = Vec::new();
    }
}

#[derive(Clone, Default)]
pub(super) struct ResponseBuffer(Arc<Mutex<State>>);
impl ResponseBuffer {
    pub(super) fn writer(&self) -> ResponseWriter {
        ResponseWriter(Arc::clone(&self.0))
    }
    pub(super) fn text(&self) -> io::Result<String> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.failed {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        if state.sealed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        state.sealed = true;
        String::from_utf8(std::mem::take(&mut state.bytes))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}

pub(super) struct ResponseWriter(Arc<Mutex<State>>);
impl Write for ResponseWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.sealed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if state.failed {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        if bytes.len() > RESPONSE_BYTES.saturating_sub(state.bytes.len()) {
            state.refuse();
            return Err(io::ErrorKind::WouldBlock.into());
        }
        if state.bytes.try_reserve_exact(bytes.len()).is_err() {
            state.refuse();
            return Err(io::Error::other("HTTP response allocation failed"));
        }
        if state.bytes.capacity() > RESPONSE_BYTES {
            state.refuse();
            return Err(io::ErrorKind::WouldBlock.into());
        }
        state.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_response_buffer_accepts_exact_capacity_and_seals_after_observation() {
        let buffer = ResponseBuffer::default();
        let mut writer = buffer.writer();
        writer.write_all(&vec![b'x'; RESPONSE_BYTES / 2]).unwrap();
        writer.write_all(&vec![b'x'; RESPONSE_BYTES / 2]).unwrap();
        assert_eq!(buffer.0.lock().unwrap().bytes.capacity(), RESPONSE_BYTES);
        assert_eq!(buffer.text().unwrap().len(), RESPONSE_BYTES);
        assert_eq!(buffer.0.lock().unwrap().bytes.capacity(), 0);
        assert_eq!(
            writer.write_all(b"late").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
    #[test]
    fn http_response_buffer_discards_exact_limit_plus_one_and_refuses_suffixes() {
        let buffer = ResponseBuffer::default();
        let mut writer = buffer.writer();
        writer.write_all(&vec![b'x'; RESPONSE_BYTES]).unwrap();
        assert_eq!(
            writer.write_all(b"x").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(buffer.0.lock().unwrap().bytes.capacity(), 0);
        assert_eq!(buffer.text().unwrap_err().kind(), io::ErrorKind::WouldBlock);
        assert_eq!(
            writer.write_all(b"{}\n").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn http_protocol_notifier_refuses_aggregate_overflow_without_publishing_a_prefix() {
        use crate::mcp::notify::Notifier;
        use fsm_core::{canon::canon_bytes, json::Value};
        use std::collections::BTreeMap;
        const EXPECTED_LIMIT: usize = 8 * 1024 * 1024;
        let buffer = ResponseBuffer::default();
        let notifier = Notifier::new(Box::new(buffer.writer()));
        let mut response = Value::Obj(BTreeMap::from([
            ("jsonrpc".into(), Value::Str("2.0".into())),
            ("id".into(), Value::Num("1".into())),
            ("result".into(), Value::Str(String::new())),
        ]));
        let framing = canon_bytes(&response).len() + 1;
        if let Value::Obj(fields) = &mut response {
            fields.insert(
                "result".into(),
                Value::Str("x".repeat(EXPECTED_LIMIT - framing)),
            );
        }
        notifier.send(&response).unwrap();
        assert_eq!(buffer.0.lock().unwrap().bytes.capacity(), EXPECTED_LIMIT);
        assert_eq!(
            notifier.send(&Value::Null).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert!(notifier.is_broken());
        assert_eq!(buffer.0.lock().unwrap().bytes.capacity(), 0);
        assert_eq!(buffer.text().unwrap_err().kind(), io::ErrorKind::WouldBlock);
    }
}
