//! Native ownership retained across temporary paired writer availability.
use super::{closures::Closures, control::ExecutorControl};
use crate::{
    config::HandlerTable,
    error::ExecError,
    run::{Pipeline, Runner},
    sched::Scheduler,
    watch::Watcher,
};
use fsm_store::{clock::Clock, store::Store};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub struct PairedNativeExecutor {
    directory: PathBuf,
    physical: (u64, u64),
    snapshot: Store,
    watcher: Watcher,
    scheduler: Scheduler,
    runner: Runner,
    pipeline: Pipeline,
    control: ExecutorControl,
    closures: Closures,
}
impl PairedNativeExecutor {
    /// Open a verified read-only prefix; never acquire or initialize a writer.
    pub fn new(directory: &Path, table: HandlerTable) -> Result<Self, ExecError> {
        let snapshot =
            Store::open_read_only(directory).map_err(|error| ExecError::store(&error))?;
        let watcher = Watcher::with_handlers(directory.to_path_buf(), &table);
        Self::from_owned_parts(
            snapshot,
            watcher,
            Scheduler::new(table),
            Runner::new_native()?,
        )
    }
    /// Retain the exact original native components with a verified reader prefix.
    pub fn from_owned_parts(
        snapshot: Store,
        watcher: Watcher,
        scheduler: Scheduler,
        runner: Runner,
    ) -> Result<Self, ExecError> {
        let admission = runner.native_admission_control()?;
        if snapshot.journal.is_memory()
            || !snapshot.journal.is_read_only()
            || snapshot.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "paired lifecycle requires a verified durable read-only snapshot",
            ));
        }
        let metadata = std::fs::metadata(&snapshot.data_dir).map_err(|_| unproven())?;
        if !metadata.is_dir() {
            return Err(unproven());
        }
        let driver = Self {
            directory: snapshot.data_dir.clone(),
            physical: (metadata.dev(), metadata.ino()),
            snapshot,
            watcher,
            scheduler,
            runner,
            pipeline: Pipeline,
            control: ExecutorControl::new(admission),
            closures: Closures::default(),
        };
        driver.publish(false);
        Ok(driver)
    }
    pub fn control(&self) -> ExecutorControl {
        self.control.clone()
    }
    pub fn data_dir(&self) -> &Path {
        &self.directory
    }
    pub fn physical_store_identity(&self) -> (u64, u64) {
        self.physical
    }
    pub fn handler_table(&self) -> &HandlerTable {
        self.scheduler.handler_table()
    }

    /// Only explicit ticks may schedule new pending work/retries/deadlines.
    pub fn tick(&mut self, clock: &mut dyn Clock, now_ms: i64) -> Vec<String> {
        if self.control.requested() {
            return self.poll(clock, now_ms);
        }
        if let Err(error) = self.check_physical() {
            return vec![super::super::error_line(&error)];
        }
        self.publish_writer(false, false);
        let mut lines = super::super::tick_reporting(
            &mut self.watcher,
            &mut self.scheduler,
            &mut self.runner,
            &mut self.pipeline,
            &self.directory,
            clock,
            now_ms,
        )
        .lines;
        if self.control.requested() {
            lines.extend(self.poll(clock, now_ms));
        } else {
            let complete = self.refresh(now_ms, &mut lines);
            self.publish(complete);
        }
        lines
    }

    /// Observe/close admitted local work before considering temporary writer I/O.
    pub fn poll(&mut self, clock: &mut dyn Clock, now_ms: i64) -> Vec<String> {
        if self.control.report().phase == super::ExecutorPhase::Stopped {
            return Vec::new();
        }
        let mut lines = Vec::new();
        self.runner.finished_effects();
        self.runner.release_native_preparations(&mut self.scheduler);
        let mut complete = self.refresh(now_ms, &mut lines);
        if self.control.requested() {
            self.runner.cancel_foreign_native_helpers();
        }
        if self.control.closure_requested() {
            // The retained original verified snapshot permits closure even when
            // a current writer cannot be acquired; physical/hash guards remain.
            self.closures
                .start(&self.runner, &self.snapshot, &mut lines);
        }
        self.closures.poll(
            &mut self.runner,
            &mut self.scheduler,
            None,
            clock,
            &mut lines,
        );
        let (ids, unclaimed, helpers_retired) = self.runner.native_shutdown_inventory();
        let empty_stop = self.control.requested()
            && ids.is_empty()
            && unclaimed == 0
            && helpers_retired
            && self.closures.is_empty();
        if !empty_stop && complete {
            self.publish_writer(complete, false);
            match Store::open(&self.directory) {
                Ok(mut writer) => {
                    match super::super::observe_admitted_with(
                        &mut self.watcher,
                        &mut self.scheduler,
                        &mut self.runner,
                        &mut self.pipeline,
                        &mut writer,
                        clock,
                        now_ms,
                    ) {
                        Ok(observed) => lines.extend(observed),
                        Err(error) => {
                            complete = false;
                            lines.push(super::super::error_line(&error));
                        }
                    }
                    self.closures.poll(
                        &mut self.runner,
                        &mut self.scheduler,
                        Some(&mut writer),
                        clock,
                        &mut lines,
                    );
                    // Actual writer release precedes publishing any Stopped fact.
                    drop(writer);
                }
                Err(error) => {
                    lines.push(super::super::error_line(&ExecError::store(&error)));
                    // Existing local IDs/reservations remain charged; absence of
                    // writer access never retires or settles original ownership.
                }
            }
        }
        self.publish(complete);
        lines
    }
    fn check_physical(&self) -> Result<(), ExecError> {
        let metadata = std::fs::metadata(&self.directory).map_err(|_| unproven())?;
        if (metadata.dev(), metadata.ino()) != self.physical {
            return Err(unproven());
        }
        Ok(())
    }
    fn refresh(&mut self, now_ms: i64, lines: &mut Vec<String>) -> bool {
        let result = (|| {
            self.check_physical()?;
            let (mut observation, snapshot) = self.watcher.scan_snapshot(now_ms)?;
            if snapshot.journal.poisoned {
                return Err(unproven());
            }
            let metadata = std::fs::metadata(&snapshot.data_dir).map_err(|_| unproven())?;
            if (metadata.dev(), metadata.ino()) != self.physical {
                return Err(unproven());
            }
            self.runner
                .recover_native_owners(&snapshot, &mut observation, &mut self.scheduler)?;
            lines.extend(observation.unresolved.iter().map(super::super::error_line));
            self.snapshot = snapshot;
            Ok(())
        })();
        match result {
            Ok(()) => true,
            Err(error) => {
                lines.push(super::super::error_line(&error));
                false
            }
        }
    }
    fn publish(&self, complete: bool) {
        self.publish_writer(complete, true);
    }
    fn publish_writer(&self, complete: bool, writer_released: bool) {
        let (ids, unclaimed, retired) = self.runner.native_shutdown_inventory();
        self.control.publish(
            ids,
            unclaimed,
            retired && self.closures.is_empty(),
            complete,
            writer_released,
        );
    }
}
fn unproven() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "paired lifecycle requires the original verified physical store",
    )
}
