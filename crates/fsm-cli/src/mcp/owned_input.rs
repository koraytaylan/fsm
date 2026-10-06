//! One owned, bounded protocol reader; independent stop never joins quiet stdin.
use super::framing::{LINE_CAP, Line, read_capped_line};
use std::{
    io::{self, BufRead, Read},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::Duration,
};

const IDLE_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub(super) enum FrameSignal {
    Idle,
    TooLong,
}
impl std::fmt::Display for FrameSignal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Idle => "protocol input idle",
            Self::TooLong => "protocol input frame too long",
        })
    }
}
impl std::error::Error for FrameSignal {}

pub(super) struct OwnedInput {
    frames: Receiver<io::Result<Line>>,
    bytes: Vec<u8>,
    consumed: usize,
    newline: bool,
    eof: bool,
    stop_requested: Box<dyn Fn() -> bool>,
}
impl OwnedInput {
    // Reader is constructed inside its sole worker; borrowed public input APIs
    // acquire no Send bound. Queue one + current one + worker one bounds frames.
    pub(super) fn start<R: BufRead + 'static>(
        reader: impl FnOnce() -> R + Send + 'static,
        stop_requested: impl Fn() -> bool + 'static,
    ) -> io::Result<Self> {
        let (sender, frames) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("fsm-protocol-input".into())
            .spawn(move || {
                let mut reader = reader();
                loop {
                    let frame = read_capped_line(&mut reader, LINE_CAP);
                    let terminal = matches!(&frame, Ok(Line::Eof) | Err(_));
                    if sender.send(frame).is_err() || terminal {
                        break;
                    }
                }
            })?;
        Ok(Self {
            frames,
            bytes: Vec::new(),
            consumed: 0,
            newline: false,
            eof: false,
            stop_requested: Box::new(stop_requested),
        })
    }
}
impl Read for OwnedInput {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let available = self.fill_buf()?;
        let count = available.len().min(buffer.len());
        buffer[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}
impl BufRead for OwnedInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if (self.stop_requested)() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "explicit lifecycle stop requested",
            ));
        }
        if self.consumed < self.bytes.len() {
            return Ok(&self.bytes[self.consumed..]);
        }
        // Emit LF separately: pushing it onto an exact-cap Vec could double
        // allocation capacity, and an oversize sentinel must allocate no frame.
        if self.newline {
            return Ok(b"\n");
        }
        if self.eof {
            return Ok(&[]);
        }
        self.bytes = Vec::new();
        self.consumed = 0;
        match self.frames.recv_timeout(IDLE_INTERVAL) {
            Ok(Ok(Line::Data(bytes))) => {
                self.bytes = bytes;
                self.newline = true;
                if self.bytes.is_empty() {
                    Ok(b"\n")
                } else {
                    Ok(&self.bytes)
                }
            }
            Ok(Ok(Line::Idle)) => Err(io::Error::new(io::ErrorKind::WouldBlock, FrameSignal::Idle)),
            Ok(Ok(Line::TooLong)) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                FrameSignal::TooLong,
            )),
            Ok(Ok(Line::Eof)) => {
                self.eof = true;
                Ok(&[])
            }
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => {
                if (self.stop_requested)() {
                    Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "explicit lifecycle stop requested",
                    ))
                } else {
                    Err(io::Error::new(io::ErrorKind::WouldBlock, FrameSignal::Idle))
                }
            }
            // A vanished/panicked worker is an I/O failure, not clean protocol EOF.
            Err(RecvTimeoutError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "protocol reader exited without EOF",
            )),
        }
    }
    fn consume(&mut self, count: usize) {
        if self.consumed < self.bytes.len() {
            self.consumed += count.min(self.bytes.len() - self.consumed);
        } else if self.newline && count > 0 {
            self.newline = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Cursor,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Instant,
    };

    fn next_frame(input: &mut OwnedInput) -> Line {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let frame = read_capped_line(input, LINE_CAP).unwrap();
            if !matches!(frame, Line::Idle) {
                return frame;
            }
            assert!(
                Instant::now() < deadline,
                "owned input did not deliver a frame"
            );
        }
    }

    #[test]
    fn actual_wire_limit_and_plus_one_preserve_the_following_frame() {
        let mut wire = vec![b'a'; LINE_CAP];
        wire.push(b'\n');
        wire.extend(std::iter::repeat_n(b'b', LINE_CAP + 1));
        wire.push(b'\n');
        wire.extend(std::iter::repeat_n(b'c', LINE_CAP + 1));
        wire.extend_from_slice(b"\nnext\n");
        let mut input = OwnedInput::start(move || Cursor::new(wire), || false).unwrap();
        let Line::Data(bytes) = next_frame(&mut input) else {
            panic!("exact cap must pass")
        };
        assert_eq!(bytes.len(), LINE_CAP);
        assert!(bytes.iter().all(|byte| *byte == b'a'));
        // Inspect one worker refusal before the outer frame guard: otherwise
        // double checking could hide a disabled worker allocation guard.
        assert!(matches!(
            input
                .frames
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap(),
            Line::TooLong
        ));
        assert!(matches!(next_frame(&mut input), Line::TooLong));
        assert!(matches!(next_frame(&mut input), Line::Data(bytes) if bytes == b"next"));
        assert!(matches!(next_frame(&mut input), Line::Eof));
    }

    #[test]
    fn reverse_reply_uses_the_same_owned_reader_after_an_outer_frame() {
        let mut input =
            OwnedInput::start(|| Cursor::new(b"outer\nreverse\n".to_vec()), || false).unwrap();
        assert!(matches!(next_frame(&mut input), Line::Data(bytes) if bytes == b"outer"));
        let notifier = super::super::notify::Notifier::new(Box::new(Cursor::new(Vec::<u8>::new())));
        let mut session = super::super::notify::SessionIo::new(&notifier, &mut input);
        assert_eq!(session.read_line().unwrap().as_deref(), Some("reverse"));
        assert_eq!(session.read_line().unwrap(), None);
    }

    struct HeldReader {
        ready: Option<mpsc::Sender<()>>,
        release: Receiver<()>,
        retired: mpsc::Sender<()>,
    }
    impl Read for HeldReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            self.fill_buf().map(|_| 0)
        }
    }
    impl BufRead for HeldReader {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if let Some(ready) = self.ready.take() {
                ready.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(&[])
        }
        fn consume(&mut self, _: usize) {}
    }
    impl Drop for HeldReader {
        fn drop(&mut self) {
            let _ = self.retired.send(());
        }
    }

    #[test]
    fn actual_held_reader_does_not_hold_stop_or_reverse_reply_wait() {
        let (ready_sender, ready_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (retired_sender, retired_receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let control = stop.clone();
        let mut input = OwnedInput::start(
            move || HeldReader {
                ready: Some(ready_sender),
                release: release_receiver,
                retired: retired_sender,
            },
            move || stop.load(Ordering::Acquire),
        )
        .unwrap();
        ready_receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(next_idle(&mut input), Line::Idle));
        control.store(true, Ordering::Release);
        let notifier = super::super::notify::Notifier::new(Box::new(Cursor::new(Vec::<u8>::new())));
        let error = super::super::notify::SessionIo::new(&notifier, &mut input)
            .read_line()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        drop(input);
        release_sender.send(()).unwrap();
        retired_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
    }

    fn next_idle(input: &mut OwnedInput) -> Line {
        read_capped_line(input, LINE_CAP).unwrap()
    }
}
