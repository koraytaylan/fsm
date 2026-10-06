//! Production standalone owner composition; caller renders final errors.
use fsm_execute::{config::HandlerTable, error::ExecError, service::PairedNativeExecutor};
use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

pub(super) struct ProductionReport {
    pub execution: Result<crate::standalone::StandaloneReport, ExecError>,
    pub shutdown: fsm_execute::service::ShutdownReport,
    pub endpoint_removed: bool,
    pub endpoint_cleanup_error: Option<String>,
}

pub(super) fn run(
    data_dir: &Path,
    table: HandlerTable,
    interval_ms: u64,
    exclusive: bool,
    control_dir: Option<&str>,
) -> Result<ProductionReport, ExecError> {
    if interval_ms == 0 {
        return Err(ExecError::new(
            "exec/config",
            "standalone poll interval must be positive before publication",
        ));
    }
    let root = if let Some(directory) = control_dir {
        PathBuf::from(directory)
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            ExecError::new(
                "exec/config",
                "native executor publication requires HOME or --control-dir",
            )
        })?)
        .join(".cache/fsm/control")
    };
    // Existing roots are validated by the publisher; never silently chmod them.
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&root)
        .map_err(|error| {
            ExecError::new(
                "exec/inflight_deferred",
                format!("control root creation failed: {error}"),
            )
        })?;
    let mut driver = PairedNativeExecutor::new(data_dir, table)?;
    let endpoint = crate::local_control::LocalControlEndpoint::publish_paired(&root, &driver)
        .map_err(|error| {
            ExecError::new(
                "exec/inflight_deferred",
                format!("executor publication failed: {error}"),
            )
        })?;
    let result = crate::standalone::run_paired(
        &mut driver,
        &mut crate::clock::SystemClock,
        std::io::stdout(),
        interval_ms,
        exclusive,
        10000,
    );
    let control = driver.control();
    let cleanup_deadline = match &result {
        Ok(report) => report.shutdown_deadline,
        Err(_) => {
            // Preserve the original startup/metadata error in execution while
            // closing the actual retained owner's admission and driving abort.
            match control.stop(fsm_execute::service::ShutdownMode::Abort, 10000) {
                Ok(request) => {
                    loop {
                        let report = control.report();
                        if matches!(
                            report.phase,
                            fsm_execute::service::ExecutorPhase::Stopped
                                | fsm_execute::service::ExecutorPhase::Uncertain
                        ) {
                            break;
                        }
                        let mut clock = crate::clock::SystemClock;
                        let now_ms = fsm_store::clock::Clock::now_ms(&mut clock);
                        // No direct diagnostic I/O may suspend this cleanup path.
                        let _lines = driver.poll(&mut clock, now_ms);
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    request.deadline()
                }
                Err(_) => std::time::Instant::now(),
            }
        }
    };
    let (endpoint_removed, endpoint_cleanup_error) = match endpoint.close_until(cleanup_deadline) {
        Ok(removed) => (removed, None),
        Err(error) => (false, Some(error.to_string())),
    };
    Ok(ProductionReport {
        execution: result,
        shutdown: control.report(),
        endpoint_removed,
        endpoint_cleanup_error,
    })
}

// Classify exit without performing any synchronous output on the owner thread.
// The command renderer receives the initiating failure plus actual metadata.
pub(super) fn final_failure(report: &ProductionReport) -> Option<&ExecError> {
    match &report.execution {
        Err(error) => Some(error),
        Ok(execution) => execution.failure.as_ref(),
    }
}

pub(super) fn fully_confirmed(report: &ProductionReport) -> bool {
    final_failure(report).is_none()
        && report.shutdown.phase == fsm_execute::service::ExecutorPhase::Stopped
        && report.shutdown.admission_closed
        && report.shutdown.inventory_complete
        && report.shutdown.helpers_retired
        && report.shutdown.writer_released
        && report.shutdown.unresolved_run_ids.is_empty()
        && report.shutdown.unclaimed_reservations == Some(0)
        && report.endpoint_removed
        && report.endpoint_cleanup_error.is_none()
        && report
            .execution
            .as_ref()
            .is_ok_and(|execution| execution.output_drained && execution.dropped_lines == 0)
}
