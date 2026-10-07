//! Reverse-response mailbox: a quiet poll differs from original session closure.

use fsm_core::json::Value;
use std::sync::{Condvar, Mutex};

pub(crate) const MAX_RESPONSES: usize = 64;
pub(crate) const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// Inbound responses for one session, and whoever is waiting for them.
#[derive(Default)]
pub struct Mailbox {
    waiting: Mutex<Waiting>,
    arrived: Condvar,
}

#[derive(Default)]
struct Waiting {
    responses: std::collections::VecDeque<(Value, usize)>,
    bytes: usize,
    closed: bool,
}

impl Mailbox {
    /// Put an answer in, waking whoever is waiting.
    pub fn post(&self, message: Value) {
        if self.try_post(message).is_err() {
            self.close();
        }
    }

    pub(crate) fn try_post(&self, message: Value) -> std::io::Result<()> {
        let bytes = response_charge(&message, 0);
        let mut waiting = self.waiting.lock().unwrap_or_else(|p| p.into_inner());
        if waiting.closed {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        if waiting.responses.len() >= MAX_RESPONSES
            || bytes > MAX_RESPONSE_BYTES.saturating_sub(waiting.bytes)
        {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        waiting.bytes += bytes;
        waiting.responses.push_back((message, bytes));
        self.arrived.notify_all();
        Ok(())
    }

    /// Close this original mailbox and wake outstanding reverse waits.
    pub fn close(&self) {
        let mut waiting = self.waiting.lock().unwrap_or_else(|p| p.into_inner());
        waiting.closed = true;
        waiting.responses.clear();
        waiting.bytes = 0;
        self.arrived.notify_all();
    }

    fn is_closed(&self) -> bool {
        self.waiting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .closed
    }

    /// Take the next answer, waiting up to `timeout` for one.
    pub fn take(&self, timeout: std::time::Duration) -> Option<Value> {
        let mut waiting = self
            .waiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if waiting.responses.is_empty() && !waiting.closed {
            let (next, _) = self
                .arrived
                .wait_timeout(waiting, timeout)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            waiting = next;
        }
        waiting.responses.pop_front().map(|(message, bytes)| {
            waiting.bytes -= bytes;
            message
        })
    }
}

// Charge owned capacities plus worst-case JSON escaping, and a conservative
// 4096-byte allocation allowance per BTreeMap entry; queue slots are separately
// bounded by MAX_RESPONSES. Refuse pathological depth before recursive descent.
fn response_charge(value: &Value, depth: usize) -> usize {
    if depth > 32 {
        return usize::MAX;
    }
    let base = std::mem::size_of::<Value>();
    let content = match value {
        Value::Null => 4,
        Value::Bool(_) => 5,
        Value::Num(text) | Value::Str(text) => text
            .capacity()
            .saturating_add(text.len().saturating_mul(6))
            .saturating_add(2),
        Value::Arr(values) => values.iter().fold(
            values.capacity().saturating_mul(base).saturating_add(2),
            |bytes, child| {
                bytes
                    .saturating_add(response_charge(child, depth + 1))
                    .saturating_add(1)
            },
        ),
        Value::Obj(fields) => fields.iter().fold(2usize, |bytes, (key, child)| {
            bytes
                .saturating_add(4096)
                .saturating_add(key.capacity())
                .saturating_add(key.len().saturating_mul(6))
                .saturating_add(response_charge(child, depth + 1))
                .saturating_add(4)
        }),
    };
    base.saturating_add(content)
}

/// A reader over a mailbox, so plan 0013's `request_and_await` reads an
/// HTTP client's answer exactly as it reads a stdio one.
pub struct MailboxReader {
    mailbox: std::sync::Arc<Mailbox>,
    pending: Vec<u8>,
    at: usize,
}

impl MailboxReader {
    pub fn new(mailbox: std::sync::Arc<Mailbox>) -> Self {
        Self {
            mailbox,
            pending: Vec::new(),
            at: 0,
        }
    }
}

impl std::io::Read for MailboxReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = std::io::BufRead::fill_buf(self)?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        std::io::BufRead::consume(self, n);
        Ok(n)
    }
}

impl std::io::BufRead for MailboxReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.at == self.pending.len() {
            // One poll interval, not the whole elicitation timeout: the
            // caller's own deadline is what bounds the wait, and this loop
            // is what lets it see it.
            match self.mailbox.take(std::time::Duration::from_millis(50)) {
                Some(message) => {
                    self.pending = fsm_core::canon::canon_bytes(&message);
                    self.pending.push(b'\n');
                    self.at = 0;
                }
                None if self.mailbox.is_closed() => return Ok(&[]),
                None => return Err(std::io::ErrorKind::WouldBlock.into()),
            }
        }
        Ok(&self.pending[self.at..])
    }

    fn consume(&mut self, amount: usize) {
        self.at = (self.at + amount).min(self.pending.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::BufRead, sync::Arc, time::Duration};

    #[test]
    fn reverse_mailbox_count_boundary_rejects_without_losing_admitted_answers() {
        let mailbox = Mailbox::default();
        for _ in 0..MAX_RESPONSES {
            mailbox.try_post(Value::Null).unwrap();
        }
        assert_eq!(
            mailbox.try_post(Value::Null).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        for _ in 0..MAX_RESPONSES {
            assert_eq!(mailbox.take(Duration::ZERO), Some(Value::Null));
        }
        mailbox.try_post(Value::Null).unwrap();
        mailbox.close();
        assert_eq!(
            mailbox.try_post(Value::Null).unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn reverse_mailbox_byte_boundary_charges_owned_string_capacity() {
        let mailbox = Mailbox::default();
        let make = |extra| {
            Value::Str(String::with_capacity(
                MAX_RESPONSE_BYTES - std::mem::size_of::<Value>() - 2 + extra,
            ))
        };
        mailbox.try_post(make(0)).unwrap();
        assert_eq!(
            mailbox.try_post(Value::Null).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(mailbox.take(Duration::ZERO).is_some());
        assert_eq!(
            mailbox.try_post(make(1)).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        mailbox.try_post(make(0)).unwrap();
        mailbox.close();
        assert_eq!(mailbox.waiting.lock().unwrap().bytes, 0);
    }

    #[test]
    fn quiet_reverse_mailbox_is_idle_until_original_session_closes() {
        let mailbox = Arc::new(Mailbox::default());
        let mut reader = MailboxReader::new(Arc::clone(&mailbox));
        assert_eq!(
            reader.fill_buf().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        mailbox.close();
        assert!(reader.fill_buf().unwrap().is_empty());
        mailbox.post(Value::Str("late".into()));
        assert!(reader.fill_buf().unwrap().is_empty());
    }

    #[test]
    fn reverse_request_waits_across_quiet_polls_for_actual_answer() {
        use crate::mcp::notify::{Notifier, SessionIo, SharedSink};
        let mailbox = Arc::new(Mailbox::default());
        let sink = SharedSink::new();
        let notifier = Notifier::new(Box::new(sink.writer()));
        let responder = {
            let mailbox = Arc::clone(&mailbox);
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                while sink.text().is_empty() {
                    assert!(started.elapsed() < Duration::from_secs(2));
                    std::thread::yield_now();
                }
                let request =
                    crate::http::endpoint::json_body(sink.text().trim().as_bytes()).unwrap();
                std::thread::sleep(Duration::from_millis(150));
                mailbox.post(Value::Obj(std::collections::BTreeMap::from([
                    ("jsonrpc".into(), Value::Str("2.0".into())),
                    ("id".into(), request.get("id").unwrap().clone()),
                    ("result".into(), Value::Str("actual answer".into())),
                ])));
            })
        };
        let mut reader = MailboxReader::new(mailbox);
        let mut io = SessionIo::new(&notifier, &mut reader);
        let answer = crate::mcp::elicit::request_and_await(
            &mut io,
            "elicitation/create",
            Value::Null,
            &mut crate::clock::SystemClock,
        )
        .unwrap();
        responder.join().unwrap();
        assert_eq!(answer.as_str(), Some("actual answer"));
    }
}
