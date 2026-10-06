//! Shared protocol framing; oversize drainage never allocates the discarded tail.

use std::io::{self, BufRead};

pub(crate) const LINE_CAP: usize = 16 * 1024 * 1024;

pub(crate) enum Line {
    Eof,
    #[cfg(target_os = "linux")]
    Idle,
    TooLong,
    Data(Vec<u8>),
}

pub(crate) fn read_capped_line(
    input: &mut (impl BufRead + ?Sized),
    cap: usize,
) -> io::Result<Line> {
    let mut bytes = Vec::new();
    loop {
        let available = match input.fill_buf() {
            Ok(available) => available,
            Err(error) if bytes.is_empty() => {
                #[cfg(target_os = "linux")]
                {
                    match error
                        .get_ref()
                        .and_then(|value| value.downcast_ref::<super::owned_input::FrameSignal>())
                    {
                        Some(super::owned_input::FrameSignal::Idle) => return Ok(Line::Idle),
                        Some(super::owned_input::FrameSignal::TooLong) => return Ok(Line::TooLong),
                        None => return Err(error),
                    }
                }
                #[cfg(not(target_os = "linux"))]
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(Line::Eof)
            } else {
                Ok(Line::Data(bytes))
            };
        }
        if let Some(end) = available.iter().position(|byte| *byte == b'\n') {
            if end > cap.saturating_sub(bytes.len()) {
                input.consume(end + 1);
                return Ok(Line::TooLong);
            }
            bytes.extend_from_slice(&available[..end]);
            input.consume(end + 1);
            return Ok(Line::Data(bytes));
        }
        if available.len() > cap.saturating_sub(bytes.len()) {
            let consumed = available.len();
            input.consume(consumed);
            drop(bytes);
            discard_line(input)?;
            return Ok(Line::TooLong);
        }
        bytes.extend_from_slice(available);
        let consumed = available.len();
        input.consume(consumed);
    }
}

fn discard_line(input: &mut (impl BufRead + ?Sized)) -> io::Result<()> {
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(());
        }
        if let Some(end) = available.iter().position(|byte| *byte == b'\n') {
            input.consume(end + 1);
            return Ok(());
        }
        let consumed = available.len();
        input.consume(consumed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn exact_wire_byte_limit_accepts_and_plus_one_drains_only_original_frame() {
        let mut input = BufReader::with_capacity(2, Cursor::new(b"abcd\nabcde\nnext\n"));
        assert!(
            matches!(read_capped_line(&mut input, 4).unwrap(), Line::Data(bytes) if bytes == b"abcd")
        );
        assert!(matches!(
            read_capped_line(&mut input, 4).unwrap(),
            Line::TooLong
        ));
        assert!(
            matches!(read_capped_line(&mut input, 4).unwrap(), Line::Data(bytes) if bytes == b"next")
        );
        assert!(matches!(
            read_capped_line(&mut input, 4).unwrap(),
            Line::Eof
        ));
    }
    #[test]
    fn zero_limit_accepts_empty_frame_and_refuses_nonempty_frame() {
        let mut input = Cursor::new(b"\na\n\n");
        assert!(
            matches!(read_capped_line(&mut input, 0).unwrap(), Line::Data(bytes) if bytes.is_empty())
        );
        assert!(matches!(
            read_capped_line(&mut input, 0).unwrap(),
            Line::TooLong
        ));
        assert!(
            matches!(read_capped_line(&mut input, 0).unwrap(), Line::Data(bytes) if bytes.is_empty())
        );
    }

    #[test]
    fn unterminated_exact_limit_is_returned_once_and_oversize_is_refused() {
        let mut exact = BufReader::with_capacity(1, Cursor::new(b"abcd"));
        assert!(
            matches!(read_capped_line(&mut exact, 4).unwrap(), Line::Data(bytes) if bytes == b"abcd")
        );
        assert!(matches!(
            read_capped_line(&mut exact, 4).unwrap(),
            Line::Eof
        ));
        let mut oversized = BufReader::with_capacity(1, Cursor::new(b"abcde"));
        assert!(matches!(
            read_capped_line(&mut oversized, 4).unwrap(),
            Line::TooLong
        ));
        assert!(matches!(
            read_capped_line(&mut oversized, 4).unwrap(),
            Line::Eof
        ));
    }
    struct FailedTail {
        consumed: bool,
    }
    impl std::io::Read for FailedTail {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "tail unavailable",
            ))
        }
    }
    impl BufRead for FailedTail {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if self.consumed {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "tail unavailable",
                ))
            } else {
                Ok(b"abcde")
            }
        }
        fn consume(&mut self, count: usize) {
            assert_eq!(count, 5);
            self.consumed = true;
        }
    }

    #[test]
    fn failed_oversize_drain_propagates_original_read_error() {
        let mut input = FailedTail { consumed: false };
        match read_capped_line(&mut input, 4) {
            Err(error) => {
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                assert_eq!(error.to_string(), "tail unavailable");
            }
            _ => panic!("failed drainage must not claim frame resynchronization"),
        }
    }
}
