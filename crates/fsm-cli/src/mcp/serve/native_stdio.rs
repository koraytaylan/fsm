//! Process-scoped native stdio ownership; borrowed APIs retain their bounds.
use super::{ExecutorLoop, ServeMode, WriterUnavailable};
use fsm_execute::{
    run::Runner,
    service::{ExecutorPhase, OwnedNativeExecutor},
};
use std::{io, os::unix::fs::DirBuilderExt, path::Path};

pub(super) fn run(dir: &Path, executor: ExecutorLoop) -> io::Result<()> {
    let store = match super::open_writer(dir) {
        Ok(store) => store,
        Err(reason) => {
            let (opened, contended) = match reason {
                WriterUnavailable::Contended(store) => (Ok(*store), true),
                WriterUnavailable::Unhealthy(error) => (Err(*error), false),
            };
            // Reuse the exact observed diagnostic prefix, never reopen into a
            // legacy executor if the competing writer releases during startup.
            return super::serve_opened_with(
                dir,
                ServeMode::Embedded(Box::new(executor)),
                opened,
                contended,
                io::stdin().lock(),
                io::stdout(),
            );
        }
    };
    let runner = Runner::new_native().map_err(|error| io::Error::other(error.message))?;
    let mut driver =
        OwnedNativeExecutor::from_owned_parts(store, executor.watcher, executor.scheduler, runner)
            .map_err(|error| io::Error::other(error.message))?;
    let root = std::path::PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "native stdio publication requires HOME",
        )
    })?)
    .join(".cache/fsm/control");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&root)?;
    let endpoint = crate::local_control::LocalControlEndpoint::publish(&root, &mut driver)?;
    let result = super::serve_owned_native_session_reporting(
        &mut driver,
        &mut crate::clock::SystemClock,
        || io::BufReader::new(io::stdin()),
        io::stdout(),
        10000,
    );
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            // Invalid fixed options are impossible; metadata refusal still
            // closes endpoint admission without inventing cleanup evidence.
            let _ = endpoint.close_until(std::time::Instant::now());
            return Err(error);
        }
    };
    let removal = endpoint.close_until(report.shutdown_deadline);
    if let Some(error) = report.failure {
        return Err(error);
    }
    let removed = removal?;
    let shutdown = report.shutdown;
    if shutdown.phase == ExecutorPhase::Stopped
        && shutdown.admission_closed
        && shutdown.inventory_complete
        && shutdown.helpers_retired
        && shutdown.writer_released
        && shutdown.unresolved_run_ids.is_empty()
        && shutdown.unclaimed_reservations == Some(0)
        && report.output_drained
        && removed
    {
        Ok(())
    } else {
        Err(io::Error::other(
            "native stdio shutdown or endpoint/output retirement remains uncertain",
        ))
    }
}
