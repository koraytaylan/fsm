//! Capture transport: bounded polling on Linux, historical files elsewhere.

use std::path::Path;
#[cfg(not(target_os = "linux"))]
use std::path::PathBuf;
use std::process::Stdio;

use super::{BoundedBytes, ExecError};

#[cfg(target_os = "linux")]
use {
    super::{capture::Capture, spawn_error},
    std::io::{ErrorKind, Read},
    std::os::{fd::OwnedFd, unix::net::UnixStream},
};

pub(super) struct StreamCapture {
    #[cfg(target_os = "linux")]
    reader: UnixStream,
    #[cfg(target_os = "linux")]
    capture: Capture,
    #[cfg(target_os = "linux")]
    ended: bool,
    #[cfg(target_os = "linux")]
    complete: bool,
    #[cfg(not(target_os = "linux"))]
    path: PathBuf,
}

impl StreamCapture {
    pub(super) fn open(path: &Path, command: &str) -> Result<(Self, Stdio), ExecError> {
        #[cfg(target_os = "linux")]
        {
            let _ = path;
            let (reader, writer) =
                UnixStream::pair().map_err(|error| spawn_error(command, &error.to_string()))?;
            reader
                .set_nonblocking(true)
                .map_err(|error| spawn_error(command, &error.to_string()))?;
            Ok((
                Self {
                    reader,
                    capture: Capture::new(),
                    ended: false,
                    complete: false,
                },
                Stdio::from(OwnedFd::from(writer)),
            ))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let file = super::create_capture(path, command)?;
            Ok((Self { path: path.into() }, Stdio::from(file)))
        }
    }

    /// Bound work per tick independently of whether a producer stops writing.
    pub(super) fn poll(&mut self) {
        #[cfg(target_os = "linux")]
        self.drain(64 * 1024);
    }

    #[cfg(target_os = "linux")]
    fn drain(&mut self, budget: usize) {
        let mut chunk = [0; 8192];
        let mut remaining = budget;
        while !self.ended && remaining > 0 {
            let requested = remaining.min(chunk.len());
            match self.reader.read(&mut chunk[..requested]) {
                Ok(0) => {
                    self.ended = true;
                    self.complete = true;
                }
                Ok(count) => {
                    self.capture.push(&chunk[..count]);
                    remaining -= count;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // Interrupted reads consume a bounded iteration rather than
                // allowing a signal storm to monopolize a tick.
                Err(error) if error.kind() == ErrorKind::Interrupted => break,
                Err(_) => self.ended = true,
            }
        }
    }

    /// A root exit does not imply EOF: descendants can retain the writer.
    /// Final draining has a fixed budget and never waits for those peers.
    pub(super) fn finish(self) -> BoundedBytes {
        #[cfg(target_os = "linux")]
        {
            let mut this = self;
            this.drain(super::MAX_CAPTURE_READ_BYTES);
            this.capture.finish(this.complete)
        }
        #[cfg(not(target_os = "linux"))]
        {
            super::take_capture(&self.path)
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{Duration, Instant};

    #[test]
    fn retained_peer_cannot_block_capture_finish_or_earn_a_digest() {
        let (reader, mut peer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let mut stream = StreamCapture {
            reader,
            capture: Capture::new(),
            ended: false,
            complete: false,
        };
        peer.write_all(&[b'x'; super::super::ACK_OUTPUT_CAP + 1])
            .unwrap();
        stream.poll();
        let started = Instant::now();
        let result = stream.finish();
        assert!(started.elapsed() < Duration::from_millis(250));
        assert_eq!(result.bytes.len(), super::super::ACK_OUTPUT_CAP);
        assert!(result.truncated);
        assert_eq!(result.sha256, None);
        // Peer remains owned and open until after observing the result.
        drop(peer);
    }
}

#[cfg(not(target_os = "linux"))]
impl Drop for StreamCapture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
