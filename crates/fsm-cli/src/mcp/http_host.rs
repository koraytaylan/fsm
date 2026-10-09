//! HTTP requests cross the same bounded committing boundary as owned stdio.
use super::{
    host::{Handle, Owner, Session},
    notify::{Notifier, SessionIo},
    serve::Live,
};
use crate::{
    clock::{Clock, SystemClock},
    store::Store,
};
use fsm_core::json::Value;
use std::{
    cell::RefCell,
    io,
    path::PathBuf,
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(crate) struct SharedWriter {
    handle: Handle,
    data_dir: PathBuf,
    worker: Option<JoinHandle<()>>,
}

struct Retirement(Handle);
impl Drop for Retirement {
    fn drop(&mut self) {
        self.0.stop();
    }
}

impl SharedWriter {
    pub(crate) fn start(store: Store) -> io::Result<Self> {
        let data_dir = store.data_dir.clone();
        let (owner, handle) = Owner::new(store, SystemClock);
        let retirement = Retirement(handle.clone());
        let worker = std::thread::Builder::new()
            .name("fsm-http-writer".into())
            .spawn(move || {
                let _retirement = retirement;
                owner.run();
            })?;
        Ok(Self {
            handle,
            data_dir,
            worker: Some(worker),
        })
    }
    pub(crate) fn session(&self) -> io::Result<HostedSession> {
        self.handle
            .session()
            .map(|session| HostedSession {
                session,
                data_dir: self.data_dir.clone(),
            })
            .map_err(|error| io::Error::other(format!("HTTP host session admission: {error:?}")))
    }
}

impl Drop for SharedWriter {
    fn drop(&mut self) {
        self.handle.stop();
        if let Some(worker) = self.worker.take() {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !worker.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

pub(crate) struct HostedSession {
    session: Session,
    data_dir: PathBuf,
}
impl HostedSession {
    pub(crate) fn is_retired(&self) -> bool {
        self.session.is_retired()
    }
    pub(crate) fn close(&self) {
        self.session.close();
    }
    pub(crate) fn cancel(&self, id: &Value) {
        self.session.cancel(id);
    }
    #[allow(clippy::too_many_arguments)] // Preserve the common protocol entry boundary.
    pub(crate) fn dispatch<'a>(
        &'a self,
        output: &'a Notifier,
        clock: &mut dyn Clock,
        live: &mut Live,
        id: Value,
        method: &str,
        params: Option<Value>,
        mode_note: &'static str,
        io: &'a RefCell<SessionIo<'a>>,
        feed: &'a Notifier,
    ) -> io::Result<()> {
        super::methods::handle_request_hosted(
            output,
            &self.session,
            &self.data_dir,
            clock,
            &mut true,
            live,
            id,
            method,
            params,
            mode_note,
            Some(io),
            Some(feed),
        )
    }
}
impl Drop for HostedSession {
    fn drop(&mut self) {
        self.close();
    }
}
