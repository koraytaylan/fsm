//! One writer, one line at a time.
//!
//! `stdout` is the protocol stream, and a line spliced by two writers is a
//! protocol error a client cannot recover from — so the whole of this suite
//! is about the lock scope.
//!
//! Plan 0012 task 5701.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

use fsm_cli::mcp::notify::{Notifier, SharedSink};
use fsm_core::json::{JsonLimits, Value, parse};

fn message(index: usize) -> Value {
    Value::Obj(BTreeMap::from([
        ("jsonrpc".to_string(), Value::Str("2.0".into())),
        ("method".into(), Value::Str("notifications/message".into())),
        (
            "params".into(),
            Value::Obj(BTreeMap::from([(
                "data".to_string(),
                Value::Num(index.to_string()),
            )])),
        ),
    ]))
}

#[test]
fn one_send_is_one_line_and_it_is_flushed() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    notifier.send(&message(1)).unwrap();
    let text = sink.text();
    assert_eq!(text.matches('\n').count(), 1);
    assert!(text.ends_with('\n'));
    parse(text.trim_end().as_bytes(), &JsonLimits::DEFAULT).expect("one canonical message");
}

#[test]
fn eight_writers_never_splice_a_line() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut threads = Vec::new();
    for writer in 0..8 {
        let handle = notifier.clone_handle();
        threads.push(std::thread::spawn(move || {
            for index in 0..500 {
                handle.send(&message(writer * 1000 + index)).unwrap();
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    let text = sink.text();
    let mut seen = std::collections::BTreeSet::new();
    for line in text.lines() {
        let parsed = parse(line.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|error| panic!("spliced line {line:?}: {error:?}"));
        let data = parsed
            .get("params")
            .and_then(|params| params.get("data"))
            .and_then(Value::as_num)
            .unwrap_or_else(|| panic!("truncated line {line:?}"))
            .to_string();
        assert!(seen.insert(data.clone()), "duplicated {data}");
    }
    assert_eq!(seen.len(), 8 * 500, "every message arrived exactly once");
}

#[test]
fn a_notification_carries_no_id() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    notifier
        .notify(
            "notifications/resources/updated",
            Value::Obj(BTreeMap::from([(
                "uri".to_string(),
                Value::Str("fsm://instance/inst-1".into()),
            )])),
        )
        .unwrap();
    let parsed = parse(sink.text().trim_end().as_bytes(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(parsed.get("jsonrpc").and_then(Value::as_str), Some("2.0"));
    assert_eq!(
        parsed.get("method").and_then(Value::as_str),
        Some("notifications/resources/updated")
    );
    assert!(parsed.get("params").is_some());
    assert!(
        parsed.get("id").is_none(),
        "an id would make it a request, and a client that answers it is answering nobody"
    );
}

#[test]
fn a_poisoned_lock_does_not_end_the_session() {
    let sink = SharedSink::new();
    let notifier = Arc::new(Notifier::new(Box::new(sink.writer())));
    let poisoner = Arc::clone(&notifier);
    let _ = std::thread::spawn(move || {
        // Panic while a send is in flight, poisoning the lock.
        poisoner
            .send(&Value::Obj(BTreeMap::from([(
                "poison".to_string(),
                Value::Bool(true),
            )])))
            .unwrap();
        panic!("deliberate");
    })
    .join();
    notifier.send(&message(7)).expect("the stream still works");
    assert!(sink.text().contains("\"data\":7"));
}

/// A writer that fails, to prove a broken stream is reported and not
/// panicked over.
struct Broken;

impl Write for Broken {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("the client is gone"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_closed_stream_is_reported_rather_than_fatal() {
    let notifier = Notifier::new(Box::new(Broken));
    assert!(!notifier.is_broken());
    assert!(notifier.send(&message(1)).is_err());
    assert!(
        notifier.is_broken(),
        "a background producer stops rather than retrying into a stream that is gone"
    );
}

#[test]
fn a_newline_inside_a_value_still_occupies_one_line() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    notifier
        .send(&Value::Obj(BTreeMap::from([(
            "message".to_string(),
            Value::Str("first\nsecond".into()),
        )])))
        .unwrap();
    let text = sink.text();
    assert_eq!(text.matches('\n').count(), 1, "{text:?}");
    let parsed = parse(text.trim_end().as_bytes(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        parsed.get("message").and_then(Value::as_str),
        Some("first\nsecond"),
        "the escape round-trips"
    );
}

#[test]
fn two_handles_write_to_one_stream() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let other = notifier.clone_handle();
    notifier.send(&message(1)).unwrap();
    other.send(&message(2)).unwrap();
    assert_eq!(sink.text().lines().count(), 2);
}
