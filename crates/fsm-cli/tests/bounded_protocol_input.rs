//! Actual server and reverse-protocol framing under chunked oversized input.

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::{
    notify::{Notifier, SessionIo, SharedSink},
    serve::serve_session,
};
use std::io::{self, BufRead, Cursor, Read};

const CAP: usize = 16 * 1024 * 1024;

struct ChunkedInput(Cursor<Vec<u8>>);
impl Read for ChunkedInput {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}
impl BufRead for ChunkedInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let bytes = self.0.fill_buf()?;
        Ok(&bytes[..bytes.len().min(4096)])
    }
    fn consume(&mut self, count: usize) {
        self.0.consume(count);
    }
    fn read_until(&mut self, _: u8, _: &mut Vec<u8>) -> io::Result<usize> {
        panic!("unbounded tail accumulation cannot enforce the protocol budget")
    }
    fn read_line(&mut self, _: &mut String) -> io::Result<usize> {
        panic!("unbounded reverse-protocol accumulation cannot enforce the budget")
    }
}

#[test]
fn production_server_drains_chunked_oversize_and_preserves_next_frame() {
    let mut bytes = vec![b'x'; CAP + 1];
    bytes.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n");
    let sink = SharedSink::new();
    let mut clock = FixedClock::new(0, 1);
    serve_session(
        None,
        &mut clock,
        ChunkedInput(Cursor::new(bytes)),
        sink.writer(),
    )
    .unwrap();
    let text = sink.text();
    assert_eq!(text.lines().count(), 2);
    assert!(text.contains("line exceeds 16777216 bytes"));
    assert!(text.contains("\"id\":7"));
    assert!(text.contains("\"result\":{}"));
}

#[test]
fn reverse_protocol_accepts_exact_cap_refuses_plus_one_and_preserves_next_frame() {
    let mut bytes = vec![b'x'; CAP];
    bytes.push(b'\n');
    bytes.extend(std::iter::repeat_n(b'x', CAP + 1));
    bytes.extend_from_slice(b"\nnext\n");
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut input = ChunkedInput(Cursor::new(bytes));
    let mut session = SessionIo::new(&notifier, &mut input);
    assert_eq!(session.read_line().unwrap().unwrap().len(), CAP);
    let error = session.read_line().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), "protocol line exceeds 16777216 bytes");
    assert_eq!(session.read_line().unwrap().as_deref(), Some("next"));
    assert!(session.read_line().unwrap().is_none());
}
