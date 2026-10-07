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
    let runner = Runner::new_native().map_err(io::Error::other)?;
    let mut driver =
        OwnedNativeExecutor::from_owned_parts(store, executor.watcher, executor.scheduler, runner)
            .map_err(io::Error::other)?;
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
    let endpoint = crate::local_control::LocalControlEndpoint::publish(&root, &mut driver)
        .map_err(|error| {
            io::Error::other(fsm_execute::error::ExecError::new(
                "exec/inflight_deferred",
                format!("native stdio publication failed: {error}"),
            ))
        })?;
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
    let removed = removal.as_ref().copied().unwrap_or(false);
    let shutdown = &report.shutdown;
    let confirmed = shutdown.phase == ExecutorPhase::Stopped
        && shutdown.admission_closed
        && shutdown.inventory_complete
        && shutdown.helpers_retired
        && shutdown.writer_released
        && shutdown.unresolved_run_ids.is_empty()
        && shutdown.unclaimed_reservations == Some(0)
        && report.output_drained
        && report.operator_output_drained
        && report.operator_lines_dropped == 0
        && removed;
    if report.failure.is_none() && confirmed {
        return Ok(());
    }
    use fsm_core::json::Value;
    use std::collections::BTreeMap;
    let kind = report
        .failure
        .as_ref()
        .map_or(io::ErrorKind::Other, io::Error::kind);
    let message = report.failure.as_ref().map_or_else(
        || "native stdio shutdown or endpoint/output retirement remains uncertain".into(),
        |error| error.to_string(),
    );
    let phase = match shutdown.phase {
        ExecutorPhase::Running => "running",
        ExecutorPhase::Draining => "draining",
        ExecutorPhase::Stopping => "stopping",
        ExecutorPhase::Stopped => "stopped",
        ExecutorPhase::Uncertain => "uncertain",
    };
    let error = fsm_execute::error::ExecError::new("exec/inflight_deferred", message).details(
        Value::Obj(BTreeMap::from([
            ("phase".into(), Value::Str(phase.into())),
            (
                "admission_closed".into(),
                Value::Bool(shutdown.admission_closed),
            ),
            ("timed_out".into(), Value::Bool(shutdown.timed_out)),
            (
                "inventory_complete".into(),
                Value::Bool(shutdown.inventory_complete),
            ),
            (
                "helpers_retired".into(),
                Value::Bool(shutdown.helpers_retired),
            ),
            (
                "writer_released".into(),
                Value::Bool(shutdown.writer_released),
            ),
            (
                "unresolved_run_ids".into(),
                Value::Arr(
                    shutdown
                        .unresolved_run_ids
                        .iter()
                        .map(|id| Value::Str(id.to_string()))
                        .collect(),
                ),
            ),
            (
                "unclaimed_reservations".into(),
                shutdown
                    .unclaimed_reservations
                    .map_or(Value::Null, |count| Value::Str(count.to_string())),
            ),
            ("endpoint_removed".into(), Value::Bool(removed)),
            (
                "endpoint_cleanup_error".into(),
                removal
                    .err()
                    .map_or(Value::Null, |error| Value::Str(error.to_string())),
            ),
            ("output_drained".into(), Value::Bool(report.output_drained)),
            (
                "operator_output_drained".into(),
                Value::Bool(report.operator_output_drained),
            ),
            (
                "operator_lines_dropped".into(),
                Value::Str(report.operator_lines_dropped.to_string()),
            ),
            (
                "initiating_io_kind".into(),
                report.failure.as_ref().map_or(Value::Null, |error| {
                    Value::Str(format!("{:?}", error.kind()))
                }),
            ),
        ])),
    );
    Err(io::Error::new(
        kind,
        crate::native_error::NativeSessionFailure {
            error,
            deadline: report.shutdown_deadline,
        },
    ))
}
