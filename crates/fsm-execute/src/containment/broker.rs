//! Provisioned local control transport; every launch still uses the claimed runner.

use super::{
    allocator, bind, broker_endpoint, broker_frame, closure, io, object, observation, runner, stop,
    text,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;

#[path = "broker_claim_closure.rs"]
mod claim_closure;

pub(super) fn serve(directory: &Path) -> Result<(), String> {
    let endpoint = broker_endpoint::open(directory)?;
    let stopping = Arc::new(AtomicBool::new(false));
    let mut sessions: Vec<JoinHandle<()>> = Vec::new();
    loop {
        for index in (0..sessions.len()).rev() {
            if sessions[index].is_finished() {
                let _ = sessions.swap_remove(index).join();
            }
        }
        if let Err(error) = endpoint.guard.check() {
            stopping.store(true, Ordering::Release);
            // Retain leadership and connection ownership until every session
            // has observed cancellation and its runner has returned.
            for session in sessions {
                let _ = session.join();
            }
            return Err(error);
        }
        match endpoint.listener.accept() {
            Ok((stream, _)) if sessions.len() < 8 => {
                let directory = directory.to_path_buf();
                let guard = endpoint.guard.clone();
                let session_stopping = stopping.clone();
                let session = std::thread::Builder::new()
                    .name("fsm-broker-session".into())
                    .spawn(move || {
                        session(directory, guard, session_stopping, stream);
                    })
                    .map_err(io);
                match session {
                    Ok(session) => sessions.push(session),
                    Err(error) => {
                        stopping.store(true, Ordering::Release);
                        for session in sessions {
                            let _ = session.join();
                        }
                        return Err(error);
                    }
                }
            }
            Ok((stream, _)) => drop(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => {
                stopping.store(true, Ordering::Release);
                for session in sessions {
                    let _ = session.join();
                }
                return Err(io(error));
            }
        }
    }
}

fn session(
    directory: PathBuf,
    guard: Arc<broker_endpoint::Guard>,
    stopping: Arc<AtomicBool>,
    mut stream: UnixStream,
) {
    let result = broker_frame::read(&mut stream).and_then(|request| {
        guard.check()?;
        if stopping.load(Ordering::Acquire) {
            return Err("broker admission closed".into());
        }
        let action = text(&request, "action")?;
        let payload = request.get("payload").ok_or("broker payload missing")?;
        stream.set_nonblocking(true).map_err(io)?;
        let mut trailing = [0];
        match stream.read(&mut trailing) {
            Ok(0) if action == "execute" => {
                return Err("broker disconnected before execution".into());
            }
            Ok(0) => {}
            Ok(_) => return Err("broker request has trailing bytes".into()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(io(error)),
        }
        let result = match action {
            "prepare" => allocator::prepare(&directory),
            "discard-prepared" => closure::discard_prepared(&directory, payload),
            "bind" => bind(&directory, payload).map(|_| Value::Null),
            "close-claimed" => claim_closure::close_claimed(&directory, payload),
            "recover" => runner::recover(&directory, broker_frame::allocation(payload)?),
            "observe" => observation::read(&directory, broker_frame::allocation(payload)?),
            "close" => {
                let allocation = broker_frame::allocation(payload)?;
                let _ = stop::fence(&directory, allocation);
                closure::complete(&directory, allocation).map(|_| Value::Null)
            }
            "execute" => execute(
                &directory,
                broker_frame::allocation(payload)?,
                &guard,
                &stopping,
                &mut stream,
            ),
            _ => Err("broker dispatch outside policy".into()),
        };
        guard.check()?;
        result
    });
    let (ok, result) = match result {
        Ok(value) => (true, value),
        Err(error) => (false, Value::Str(error.chars().take(1024).collect())),
    };
    let mut response = object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(ok)),
        ("result", result),
    ]);
    if canon_bytes(&response).len() > 65536 {
        response = object([
            ("format", Value::Str("fsm.native-response/1".into())),
            ("ok", Value::Bool(false)),
            (
                "result",
                Value::Str("broker response too large; journal ownership remains unsettled".into()),
            ),
        ]);
    }
    let _ = stream.set_nonblocking(false);
    let _ = broker_frame::write(&mut stream, &response);
}

fn execute(
    directory: &Path,
    allocation: u64,
    guard: &broker_endpoint::Guard,
    stopping: &AtomicBool,
    stream: &mut UnixStream,
) -> Result<Value, String> {
    stream.set_nonblocking(true).map_err(io)?;
    let cancellation = Arc::new(AtomicBool::new(false));
    let control = cancellation.clone();
    let directory = directory.to_path_buf();
    let worker = std::thread::Builder::new()
        .name("fsm-broker-runner".into())
        .spawn(move || runner::execute_cancellable(&directory, allocation, &control))
        .map_err(io)?;
    let mut authority_error = None;
    while !worker.is_finished() {
        if stopping.load(Ordering::Acquire) {
            cancellation.store(true, Ordering::Release);
        }
        if authority_error.is_none()
            && let Err(error) = guard.check()
        {
            authority_error = Some(error);
            cancellation.store(true, Ordering::Release);
        }
        let mut trailing = [0];
        match stream.read(&mut trailing) {
            Ok(_) => cancellation.store(true, Ordering::Release),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => cancellation.store(true, Ordering::Release),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = worker
        .join()
        .map_err(|_| "broker runner panicked; ownership remains uncertain")?;
    if let Some(error) = authority_error {
        return Err(error);
    }
    guard.check()?;
    result
}
