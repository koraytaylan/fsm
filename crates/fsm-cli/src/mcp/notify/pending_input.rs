//! Bounded raw input backlog while an admitted request awaits its owner.

use crate::mcp::jsonrpc::{Incoming, parse_line};
use fsm_core::json::Value;
use std::collections::VecDeque;

const MAX_FRAMES: usize = 8;
const MAX_BYTES: usize = 16 * 1024 * 1024;

struct Frame {
    line: String,
    cancelled: bool,
}

#[cfg(test)]
mod tests {
    use super::PendingInput;

    #[test]
    fn deferred_input_charges_capacity_at_sixteen_mebibytes_and_releases_on_take() {
        let mut pending = PendingInput::default();
        let line = String::with_capacity(16 * 1024 * 1024);
        assert!(pending.push(line));
        assert!(!pending.push(String::with_capacity(1)));
        assert_eq!(pending.take().unwrap().capacity(), 16 * 1024 * 1024);
        assert!(pending.push(String::with_capacity(1)));
    }
}

#[derive(Default)]
pub(crate) struct PendingInput {
    frames: VecDeque<Frame>,
    bytes: usize,
}

impl PendingInput {
    pub(crate) fn push(&mut self, line: String) -> bool {
        let Some(bytes) = self.bytes.checked_add(line.capacity()) else {
            return false;
        };
        if self.frames.len() == MAX_FRAMES || bytes > MAX_BYTES {
            return false;
        }
        self.bytes = bytes;
        self.frames.push_back(Frame {
            line,
            cancelled: false,
        });
        true
    }

    pub(crate) fn take(&mut self) -> Option<String> {
        while let Some(frame) = self.frames.pop_front() {
            self.bytes -= frame.line.capacity();
            if !frame.cancelled {
                return Some(frame.line);
            }
        }
        None
    }

    pub(crate) fn cancel(&mut self, requested: &Value) {
        for frame in &mut self.frames {
            if !frame.cancelled
                && matches!(parse_line(&frame.line), Ok(Incoming::Request { id, .. }) if &id == requested)
            {
                frame.cancelled = true;
            }
        }
    }
}
