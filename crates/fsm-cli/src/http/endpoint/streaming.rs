//! Production POST output owns a socket; dispatch never waits for socket writes.
use super::*;
use std::{
    io,
    net::{Shutdown, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct BufferedOutput(pub(super) Option<crate::mcp::notify::OutputControl>);
impl BufferedOutput {
    pub(super) fn drain(&self) -> io::Result<()> {
        if let Some(control) = &self.0 {
            control.close();
            let deadline = Instant::now() + DRAIN_TIMEOUT;
            while !control.drained() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            if !control.drained() {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        Ok(())
    }
}
impl Drop for BufferedOutput {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.close();
        }
    }
}

struct QueueRetirement {
    control: crate::mcp::notify::OutputControl,
    wake: TcpStream,
    delivered: bool,
    endpoint: std::sync::Weak<Endpoint>,
    session: Option<String>,
}

impl Drop for QueueRetirement {
    fn drop(&mut self) {
        self.control.close();
        if !self.delivered {
            let _ = self.wake.shutdown(Shutdown::Both);
            if let Some(endpoint) = self.endpoint.upgrade()
                && let Some(session) = &self.session
            {
                endpoint.sessions.close(session);
            }
        }
    }
}

struct PostWriter {
    socket: TcpStream,
    started: Arc<AtomicBool>,
    next_id: u64,
}

impl Write for PostWriter {
    fn write(&mut self, frame: &[u8]) -> io::Result<usize> {
        let data = frame
            .strip_suffix(b"\n")
            .ok_or(io::ErrorKind::InvalidData)?;
        if !self.started.swap(true, Ordering::AcqRel) {
            // Before any event, admission refusal remains an HTTP refusal.
            let busy = parse(data, &JsonLimits::DEFAULT).ok().is_some_and(|value| {
                value.get("error").and_then(|error| error.get("code"))
                    == Some(&Value::Num("-32004".into()))
            });
            if busy {
                write_response(&mut self.socket, &Response::error(503))?;
                return Ok(frame.len());
            }
            begin_stream(&mut self.socket)?;
        }
        write_event(&mut self.socket, self.next_id, data)?;
        self.next_id += 1;
        Ok(frame.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

impl EndpointHandler {
    pub(super) fn stream_post(
        &self,
        request: &Request,
        clock: &mut dyn Clock,
        output: &mut dyn Write,
        socket: TcpStream,
    ) -> io::Result<super::super::server::Flow> {
        let wake = socket.try_clone()?;
        let started = Arc::new(AtomicBool::new(false));
        let (notifier, control) = Notifier::http_hosted_queued(Box::new(PostWriter {
            socket,
            started: Arc::clone(&started),
            next_id: 1,
        }))?;
        let mut retirement = QueueRetirement {
            control: control.clone(),
            wake,
            delivered: false,
            endpoint: Arc::downgrade(&self.endpoint),
            session: request.header(SESSION_HEADER).map(str::to_owned),
        };
        let endpoint = Arc::downgrade(&self.endpoint);
        let session = request.header(SESSION_HEADER).map(str::to_owned);
        let failure_session = session.clone();
        control.on_failure(move || {
            if let Some(endpoint) = endpoint.upgrade()
                && let Some(session) = failure_session
            {
                endpoint.sessions.close(&session);
            }
        });
        // Validation refusals are written here; streamed protocol output goes
        // only through the queue, so the two cannot interleave on the socket.
        let mut refusal = Vec::new();
        let result = self
            .endpoint
            .serve_with_output(request, clock, &mut refusal, Some(&notifier));
        control.close();
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        while !control.drained() && !notifier.is_broken() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if result.is_err() || !control.drained() {
            if let Some(session) = session {
                self.endpoint.sessions.close(&session);
            }
            // Wake the actual socket worker; never join blocked I/O.
            let _ = retirement.wake.shutdown(Shutdown::Both);
            return Err(result
                .err()
                .unwrap_or_else(|| io::ErrorKind::TimedOut.into()));
        }
        if !refusal.is_empty() {
            output.write_all(&refusal)?;
        } else if !started.load(Ordering::Acquire) {
            let retired = session.as_ref().is_some_and(|id| {
                self.endpoint
                    .lives
                    .lock_safe()
                    .get(id)
                    .and_then(|state| state.hosted.as_ref())
                    .is_some_and(|host| host.is_retired())
            });
            write_response(
                output,
                &if retired {
                    Response::error(503)
                } else {
                    Response::text(202, "")
                },
            )?;
        }
        retirement.delivered = true;
        // A POST event body has no Content-Length: EOF delimits completion.
        Ok(super::super::server::Flow::Close)
    }
}
