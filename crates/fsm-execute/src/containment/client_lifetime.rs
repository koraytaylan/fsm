//! Owned helper lifetime observation, including during blocking transport I/O.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) struct Lifetime {
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Lifetime {
    pub(super) fn start() -> Result<Self, String> {
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stop = stopping.clone();
        let worker = std::thread::Builder::new()
            .name("native-client-lifetime".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut input = stdin.lock();
                while !worker_stop.load(Ordering::Acquire) {
                    match input.read(&mut [0]) {
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                            ) => {}
                        // This helper owns no journal or closure transition;
                        // process death closes every transport descriptor,
                        // including a connection blocked inside the kernel.
                        _ => std::process::exit(1),
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stopping,
            worker: Some(worker),
        })
    }
}

impl Drop for Lifetime {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
