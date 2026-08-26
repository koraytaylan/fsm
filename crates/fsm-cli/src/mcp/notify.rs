//! The one thing in this process that writes to the protocol stream.
//!
//! `stdout` is the protocol, and one stray byte inside a line is a protocol
//! error — so before the server can speak from two places at once, exactly
//! one type may write, and it holds a mutex across the **whole** line.
//!
//! Plan 0012 task 5701.

use std::io::Write;
use std::sync::{Arc, Mutex};

use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;

use super::jsonrpc::notification;

/// The protocol stream, and the lock that keeps it one line at a time.
pub struct Notifier {
    out: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Set once a write fails, so a caller can stop rather than retrying into
    /// a stream that is gone.
    broken: Arc<Mutex<bool>>,
}

impl Notifier {
    pub fn new(out: Box<dyn Write + Send>) -> Self {
        Self {
            out: Arc::new(Mutex::new(out)),
            broken: Arc::new(Mutex::new(false)),
        }
    }

    /// Another handle onto the same stream and the same lock.
    pub fn clone_handle(&self) -> Self {
        Self {
            out: Arc::clone(&self.out),
            broken: Arc::clone(&self.broken),
        }
    }

    /// Write one complete message: bytes, newline, flush, all under the lock.
    ///
    /// The lock scope **is** the correctness argument. A background thread
    /// and the request path share this stream, so a write that released the
    /// lock between the bytes and the newline — or between the newline and
    /// the flush — would let the other writer's line land inside this one,
    /// and a JSON-RPC client that reads a spliced line has no way to recover.
    pub fn send(&self, message: &Value) -> std::io::Result<()> {
        let bytes = canon_bytes(message);
        // A serialized message occupies exactly one line: the canonical
        // encoder escapes any newline inside a string, so a raw one here
        // would be a bug in the encoder rather than in the caller.
        debug_assert!(!bytes.contains(&b'\n'));
        // A poisoned lock means some other thread panicked mid-write, which
        // the panic hook already reports. Taking the stream anyway keeps a
        // server whose protocol state is otherwise fine alive.
        let mut out = self
            .out
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = out
            .write_all(&bytes)
            .and_then(|()| out.write_all(b"\n"))
            .and_then(|()| out.flush());
        if result.is_err() {
            *self
                .broken
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        }
        result
    }

    /// Send a notification: a method and params, and deliberately no `id`.
    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        self.send(&notification(method, params))
    }

    /// Whether a write has already failed.
    ///
    /// A closed stream means the client is gone; the main loop discovers that
    /// as EOF on its own, and a background producer should stop rather than
    /// unwind.
    pub fn is_broken(&self) -> bool {
        *self
            .broken
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A protocol stream a caller can read back afterwards.
///
/// The transport takes an owned `Write + Send + 'static`, because the change
/// feed writes from another thread — so a caller that wants the bytes cannot
/// simply lend a `Vec`. This is the one thing it needs instead: an owned
/// handle to write through, and a shared buffer to read from.
#[derive(Debug, Clone, Default)]
pub struct SharedSink(Arc<Mutex<Vec<u8>>>);

impl SharedSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// An owned writer onto the same buffer.
    pub fn writer(&self) -> SharedWriter {
        SharedWriter(Arc::clone(&self.0))
    }

    /// Everything written so far.
    pub fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Everything written so far, as text.
    pub fn text(&self) -> String {
        String::from_utf8(self.bytes()).expect("the protocol stream is UTF-8")
    }
}

/// The owned half of a [`SharedSink`].
#[derive(Debug, Clone)]
pub struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
