//! What goes on the wire, byte for byte.
//!
//! Plan 0015 task 6903.

use std::io::Write;
use std::sync::{Arc, Mutex};

use fsm_cli::http::response::{Response, StreamWriter, begin_stream, write_response};
use fsm_cli::mcp::notify::Notifier;
use fsm_core::json::Value;

/// A writer that records what it was given and where it was flushed, so an
/// event that sat in a buffer is visible as one.
#[derive(Clone, Default)]
struct Recording(Arc<Mutex<(Vec<u8>, Vec<usize>)>>);

impl Recording {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap().0).to_string()
    }
    /// The byte offsets at which a flush happened.
    fn flushes(&self) -> Vec<usize> {
        self.0.lock().unwrap().1.clone()
    }
}

impl Write for Recording {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut inner = self.0.lock().unwrap();
        let at = inner.0.len();
        inner.1.push(at);
        Ok(())
    }
}

#[test]
fn a_json_response_is_exactly_these_bytes() {
    let mut out = Vec::new();
    write_response(&mut out, &Response::json(200, b"{\"ok\":true}".to_vec())).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}"
    );
}

#[test]
fn a_multibyte_body_is_measured_in_bytes() {
    let body = "héllo — ok".as_bytes().to_vec();
    let expected = body.len();
    let mut out = Vec::new();
    write_response(&mut out, &Response::json(200, body)).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains(&format!("Content-Length: {expected}")),
        "a length counted in characters would truncate the body: {text}"
    );
    assert_ne!(expected, "héllo — ok".chars().count());
}

#[test]
fn an_empty_body_still_declares_its_length() {
    let mut out = Vec::new();
    write_response(&mut out, &Response::error(404)).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"), "{text}");
    assert!(text.contains("Content-Length: 9"), "{text}");
    assert!(text.ends_with("Not Found"));
}

#[test]
fn every_error_status_says_the_status_and_nothing_else() {
    for status in [
        400, 401, 403, 404, 405, 408, 409, 411, 413, 414, 431, 500, 503,
    ] {
        let mut out = Vec::new();
        write_response(&mut out, &Response::error(status)).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.starts_with(&format!("HTTP/1.1 {status} ")),
            "{status}: {text}"
        );
        // A stranger learns the status code. Not a path, not an internal
        // message, not a backtrace.
        for leak in ["/tmp", "/home", "panic", "unwrap", "src/", "backtrace"] {
            assert!(
                !text.contains(leak),
                "the {status} body leaks {leak}: {text}"
            );
        }
        assert!(text.contains("Content-Type: text/plain"), "{status}");
    }
}

#[test]
fn a_stream_declares_no_length_and_the_documented_headers() {
    let mut out = Vec::new();
    begin_stream(&mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text}");
    assert!(
        text.contains("Content-Type: text/event-stream\r\n"),
        "{text}"
    );
    assert!(text.contains("Cache-Control: no-cache\r\n"), "{text}");
    assert!(text.contains("Connection: keep-alive\r\n"), "{text}");
    assert!(
        !text.contains("Content-Length"),
        "a stream has no length to state: {text}"
    );
}

#[test]
fn an_event_is_framed_exactly_and_flushed_before_the_next() {
    let recording = Recording::default();
    let mut stream = StreamWriter::new(recording.clone(), 1);
    stream.event(b"{\"first\":true}").unwrap();
    let after_first = recording.text().len();
    stream.event(b"{\"second\":true}").unwrap();

    assert_eq!(
        recording.text(),
        "id: 1\ndata: {\"first\":true}\n\nid: 2\ndata: {\"second\":true}\n\n"
    );
    assert!(
        recording.flushes().contains(&after_first),
        "the first event was still in a buffer when the second was written: {:?}",
        recording.flushes()
    );
}

#[test]
fn a_keepalive_is_written_when_asked_and_never_otherwise() {
    let recording = Recording::default();
    let mut stream = StreamWriter::new(recording.clone(), 1);
    stream.event(b"{}").unwrap();
    assert!(!recording.text().contains("keepalive"));
    stream.keepalive().unwrap();
    assert!(
        recording.text().ends_with(": keepalive\n\n"),
        "{}",
        recording.text()
    );
    // A comment is not an event: the next event keeps the number it would
    // have had.
    stream.event(b"{}").unwrap();
    assert!(recording.text().contains("id: 2\n"), "{}", recording.text());
}

#[test]
fn a_notifier_over_a_stream_needs_no_change_to_the_notifier() {
    // The whole design in one assertion: plan 0012's notifier, unmodified,
    // holding an HTTP stream instead of stdout.
    let recording = Recording::default();
    let stream = StreamWriter::new(recording.clone(), 7);
    let notifier = Notifier::new(Box::new(stream));
    notifier
        .notify(
            "notifications/message",
            Value::Obj(std::collections::BTreeMap::from([(
                "level".to_string(),
                Value::Str("info".into()),
            )])),
        )
        .unwrap();
    let text = recording.text();
    assert!(text.starts_with("id: 7\ndata: {"), "{text}");
    assert!(text.contains("notifications/message"), "{text}");
    assert!(text.ends_with("\n\n"), "{text}");
    // One line in, one event out — no message is split across two events and
    // no two are run together.
    assert_eq!(text.matches("data: ").count(), 1, "{text}");
}

#[test]
fn a_stream_writer_is_a_write_and_frames_per_line() {
    let recording = Recording::default();
    let mut stream = StreamWriter::new(recording.clone(), 1);
    stream.write_all(b"first\nsecond\n").unwrap();
    assert_eq!(
        recording.text(),
        "id: 1\ndata: first\n\nid: 2\ndata: second\n\n"
    );
    // A partial line is not an event until its newline arrives.
    stream.write_all(b"third").unwrap();
    assert!(!recording.text().contains("third"));
    stream.write_all(b"\n").unwrap();
    assert!(recording.text().ends_with("id: 3\ndata: third\n\n"));
}
