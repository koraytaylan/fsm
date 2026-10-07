//! Final native CLI diagnostics share the original shutdown deadline.
//! Small Linux pipe diagnostics attempt atomic nonblocking delivery even at expiry.
use crate::{args::Ctx, mcp::notify::OutputControl, store::ErrorObj};
use fsm_execute::error::ExecError;
use std::{
    io,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(crate) struct NativeSessionFailure {
    pub(crate) error: ExecError,
    pub(crate) deadline: Instant,
}
impl std::fmt::Display for NativeSessionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for NativeSessionFailure {}

pub(crate) fn report_until(ctx: &Ctx, failure: &NativeSessionFailure) -> u8 {
    let error = &failure.error;
    let mut rendered = ErrorObj::new(error.code, error.message.clone());
    if let Some(hint) = &error.hint {
        rendered = rendered.hint(hint.clone());
    }
    if let Some(details) = &error.details {
        rendered = rendered.details(details.clone());
    }
    let mut frame = Vec::new();
    let code = crate::render::write_error(ctx.json, ctx.color && !ctx.json, &rendered, &mut frame);
    // A worker may deliver the frame, but deadline expiry never confirms that
    // delivery and must not hold process exit on stderr or a worker join.
    if deliver_pipe(&frame, failure.deadline).is_none() {
        if let Ok(output) = OutputControl::start(io::stderr()) {
            let _ = output.enqueue_diagnostic(frame);
            output.close();
            while !output.drained() && !output.is_broken() && Instant::now() < failure.deadline {
                let remaining = failure.deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
            }
        }
    }
    if error.code == "exec/config" { 2 } else { code }
}

// Linux UAPI PIPE_BUF=4096 and O_NONBLOCK=0x800; a separate open file
// description avoids changing the existing operator worker's file flags.
// This belongs only in the final caller renderer after native owner return.
fn deliver_pipe(frame: &[u8], deadline: Instant) -> Option<bool> {
    use std::io::Write;
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
    if frame.len() > 4096 {
        return None;
    }
    let mut pipe = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(0x800)
        .open("/proc/self/fd/2")
        .ok()?;
    if !pipe.metadata().ok()?.file_type().is_fifo() {
        return None;
    }
    loop {
        match pipe.write(frame) {
            Ok(written) => return Some(written == frame.len()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Some(false);
                }
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
            }
            Err(_) => return Some(false),
        }
    }
}
