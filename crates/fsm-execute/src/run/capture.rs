//! Bounded stream accounting shared by file and live capture readers.

use fsm_core::sha256::{Sha256, to_hex};

use super::{ACK_OUTPUT_CAP, BoundedBytes, MAX_CAPTURE_READ_BYTES};

/// Accept every drained chunk while retaining only the journal prefix.
/// Hashing stops at the work limit; draining can continue indefinitely without
/// growing retained memory or producing a digest for an unobserved suffix.
pub(super) struct Capture {
    prefix: Vec<u8>,
    hasher: Sha256,
    observed: usize,
    hash_complete: bool,
}

impl Capture {
    pub(super) fn new() -> Self {
        Self {
            prefix: Vec::with_capacity(ACK_OUTPUT_CAP),
            hasher: Sha256::new(),
            observed: 0,
            hash_complete: true,
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) {
        let retained = chunk.len().min(ACK_OUTPUT_CAP - self.prefix.len());
        self.prefix.extend_from_slice(&chunk[..retained]);
        let hash_room = MAX_CAPTURE_READ_BYTES.saturating_sub(self.observed);
        if self.hash_complete {
            self.hasher.update(&chunk[..chunk.len().min(hash_room)]);
            if chunk.len() > hash_room {
                self.hash_complete = false;
            }
        }
        self.observed = self.observed.saturating_add(chunk.len());
    }

    /// `complete` requires observed EOF, rather than a root exit or reader
    /// cancellation: retained pipes can outlive the direct child.
    pub(super) fn finish(self, complete: bool) -> BoundedBytes {
        let overflow = self.observed > ACK_OUTPUT_CAP;
        BoundedBytes {
            bytes: self.prefix,
            truncated: overflow || !complete,
            sha256: (overflow && complete && self.hash_complete)
                .then(|| to_hex(&self.hasher.finalize())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_boundaries_and_cancelled_streams_are_truthful() {
        for size in [
            0,
            ACK_OUTPUT_CAP,
            ACK_OUTPUT_CAP + 1,
            MAX_CAPTURE_READ_BYTES,
        ] {
            let input = vec![b'x'; size];
            let mut capture = Capture::new();
            for chunk in input.chunks(317) {
                capture.push(chunk);
            }
            let result = capture.finish(true);
            assert_eq!(result.bytes, input[..size.min(ACK_OUTPUT_CAP)]);
            assert_eq!(result.truncated, size > ACK_OUTPUT_CAP);
            let mut expected = Sha256::new();
            expected.update(&input);
            assert_eq!(
                result.sha256,
                (size > ACK_OUTPUT_CAP).then(|| to_hex(&expected.finalize()))
            );
        }
        let mut capture = Capture::new();
        capture.push(b"unfinished");
        let result = capture.finish(false);
        assert_eq!(result.bytes, b"unfinished");
        assert!(result.truncated);
        assert_eq!(result.sha256, None);
    }

    #[test]
    fn excess_chunks_keep_draining_without_retained_growth_or_digest() {
        let mut capture = Capture::new();
        let chunk = [b'z'; 8192];
        for _ in 0..(MAX_CAPTURE_READ_BYTES / chunk.len() + 1000) {
            capture.push(&chunk);
            assert_eq!(capture.prefix.len(), ACK_OUTPUT_CAP);
            assert_eq!(capture.prefix.capacity(), ACK_OUTPUT_CAP);
        }
        let result = capture.finish(true);
        assert!(result.truncated);
        assert_eq!(result.sha256, None);
    }
}
