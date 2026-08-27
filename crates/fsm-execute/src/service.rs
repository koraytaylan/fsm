//! The driver: one tick, and the loop around it.
//!
//! Two entry points, because two callers own the writer differently.
//! [`tick_with`] works against a writer handle it is *lent*, which is how an
//! embedded MCP server drives the same loop on the handle it already holds;
//! [`tick`] opens the writer itself, only for the ticks that write, and drops
//! it before returning, so the executor never holds the single-writer lock
//! across a sleep.
//!
//! The three composition directives — creating a child, returning its
//! result, delivering a signal — never touch the runner. A subprocess exists
//! to reach the world's computers; these reach only the journal, so they go
//! straight to the pipeline and take the writer for the tick like any other
//! write.
//!
//! Every action produces one line, and those lines carry **identifiers only** —
//! effect name, effect id, request id, event, outcome. Never a path, a pid, a
//! temporary directory, or a duration: those differ per machine and per run,
//! and the golden session byte-compares this stream.

use std::collections::BTreeMap;
use std::path::Path;

use fsm_store::clock::Clock;
use fsm_store::store::Store;

use crate::config::{Advance, HandlerTable};
use crate::effect::PendingEffect;
use crate::error::ExecError;
use crate::run::{KillReason, Pipeline, RunOutcome, Runner, SettleOutcome};
use crate::sched::{Directive, Scheduler};
use crate::watch::{Observation, Watcher};

/// The read-only half of a tick: what the journal says, and what to do about
/// it.
struct Plan {
    observation: Observation,
    directives: Vec<Directive>,
    lines: Vec<String>,
}

/// Run one tick against a writer the caller owns, returning its action lines.
pub fn tick_with(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    store: &mut Store,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> Vec<String> {
    let mut plan = match plan(watcher, scheduler, now_ms) {
        Ok(plan) => plan,
        Err(lines) => return lines,
    };
    let settles = prepare(scheduler, runner, &mut plan);
    let finished = runner.finished_effects();
    plan.lines.extend(settle_phase(
        scheduler, runner, pipeline, store, clock, &plan, settles, finished,
    ));
    plan.lines
}

/// What one tick did, for a driver that has to react to more than its lines.
pub struct TickOutcome {
    /// One line per action, identifiers only.
    pub lines: Vec<String>,
    /// This tick had writes to do and could not take the writer.
    ///
    /// Ordinary in paired mode — the MCP host or the CLI holds the lock for a
    /// moment — and a contradiction in exclusive mode, where the operator said
    /// nothing else writes here.
    pub writer_unavailable: bool,
}

/// Run one tick, opening the writer only if the tick has something to write.
///
/// `Store::open` folds the journal (or loads the newest snapshot and folds the
/// tail) and `Drop` writes a snapshot, so a tick that settles ten directives
/// pays one fold and one snapshot rather than ten. A tick with nothing to
/// write opens no writer at all — which is also what leaves the lock free for
/// the CLI or an MCP writer between ticks.
pub fn tick(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    data_dir: &Path,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> Vec<String> {
    tick_reporting(
        watcher, scheduler, runner, pipeline, data_dir, clock, now_ms,
    )
    .lines
}

/// [`tick`], with the one fact a driver cannot read off the lines.
pub fn tick_reporting(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    data_dir: &Path,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> TickOutcome {
    let mut plan = match plan(watcher, scheduler, now_ms) {
        Ok(plan) => plan,
        Err(lines) => {
            return TickOutcome {
                lines,
                writer_unavailable: false,
            };
        }
    };
    // Starting and stopping handlers writes nothing, so both happen before the
    // writer is even considered. A kill in particular must not wait on the
    // lock: a handler past its timeout has to stop whether or not this tick
    // can journal the fact.
    let settles = prepare(scheduler, runner, &mut plan);
    let finished = runner.finished_effects();
    if !writes_anything(&plan.directives, &settles, &finished) {
        return TickOutcome {
            lines: plan.lines,
            writer_unavailable: false,
        };
    }
    let mut store = match Store::open(data_dir) {
        Ok(store) => store,
        Err(error) => {
            // Contention with another writer is expected in paired mode: back
            // off and let the next tick try, rather than failing the run.
            plan.lines.push(error_line(&ExecError::store(&error)));
            // Nothing was journaled, so nothing may stay marked in flight:
            // an entry no tick can clear is invisible to the start rule for
            // the life of the process. Clearing it means the next tick runs
            // the handler again — the at-least-once boundary, taken
            // deliberately rather than wedging the loop.
            for settle in &settles {
                scheduler.complete(&settle.effect.effect_id);
            }
            return TickOutcome {
                lines: plan.lines,
                writer_unavailable: true,
            };
        }
    };
    plan.lines.extend(settle_phase(
        scheduler, runner, pipeline, &mut store, clock, &plan, settles, finished,
    ));
    TickOutcome {
        lines: plan.lines,
        writer_unavailable: false,
    }
}

/// Tick, emit, sleep, repeat — the whole executor loop, with no async runtime.
///
/// A plain `loop` with `std::thread::sleep`, matching the blocking posture of
/// the rest of the workspace. It runs until the process is stopped: a tick
/// that cannot open the writer, or whose scan fails, reports the fact and lets
/// the next tick try again, because contention with another writer is expected
/// rather than fatal.
///
/// Lines go to the caller's `emit` because this crate may not print: the
/// workspace lints deny `print_stdout` and `print_stderr` in libraries, and
/// the CLI owns the output frame anyway.
pub fn run(
    config: RunConfig<'_>,
    clock: &mut dyn Clock,
    emit: &mut dyn FnMut(&str),
) -> Result<(), ExecError> {
    let mut watcher = Watcher::new(
        config.data_dir.to_path_buf(),
        advancing_effects(&config.table),
    );
    let mut scheduler = Scheduler::new(config.table);
    let mut runner = Runner::new()?;
    let mut pipeline = Pipeline;
    let interval = std::time::Duration::from_millis(config.poll_interval_ms);
    let mut blocked_ticks = 0;
    loop {
        // One `now_ms` per tick, read once: every decision in a tick sees the
        // same instant, while the store's own mutators go on consuming clock
        // ticks as they journal.
        let now_ms = clock.now_ms();
        let outcome = tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            config.data_dir,
            clock,
            now_ms,
        );
        for line in &outcome.lines {
            emit(line);
        }
        blocked_ticks = if outcome.writer_unavailable {
            blocked_ticks + 1
        } else {
            0
        };
        if config.contention == Contention::Fail && blocked_ticks >= BLOCKED_TICKS_BEFORE_FAIL {
            // The pre-flight check at startup is a fast failure for the usual
            // case; this is the one that cannot be raced, because the check is
            // the write itself.
            return Err(ExecError::new(
                "exec/mode",
                format!(
                    "another writer has held {} for {blocked_ticks} consecutive ticks while this executor is running exclusive",
                    config.data_dir.display()
                ),
            )
            .hint("stop the other writer, or drop --exclusive to run paired beside it"));
        }
        std::thread::sleep(interval);
    }
}

/// What the loop does when it cannot take the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contention {
    /// Another writer holds the lock for a moment. Back off and try again.
    Retry,
    /// The operator said nothing else writes here. Stop and say so — but only
    /// on evidence, not on one blocked tick. See [`BLOCKED_TICKS_BEFORE_FAIL`].
    Fail,
}

/// Consecutive blocked ticks an exclusive run tolerates before ending.
///
/// One is not evidence of another writer. An advisory lock is released when
/// the last file descriptor for it closes, and this process forks to spawn
/// every handler: between `fork` and `exec` the child holds a copy of whatever
/// was open, so a lock this process just dropped can stay held for the length
/// of that window. Ending a run over a microsecond of its own making would be
/// worse than useless.
pub const BLOCKED_TICKS_BEFORE_FAIL: u32 = 3;

/// The effect names whose handler declares an advance for some outcome.
///
/// The watcher uses it to keep acks that can never need an advance out of its
/// outstanding list; the table is the only place that knows.
pub fn advancing_effects(table: &HandlerTable) -> std::collections::BTreeSet<String> {
    table
        .handlers
        .values()
        .filter(|handler| handler.on_ok.is_some() || handler.on_failed.is_some())
        .map(|handler| handler.effect.clone())
        .collect()
}

/// Everything [`run`] needs that is not the clock or the output sink.
pub struct RunConfig<'a> {
    /// The data directory to watch and write.
    pub data_dir: &'a Path,
    /// The validated handler table.
    pub table: HandlerTable,
    /// Milliseconds between ticks.
    pub poll_interval_ms: u64,
    /// What to do about another writer.
    pub contention: Contention,
}

/// Scan, then decide. Nothing here writes or spawns.
fn plan(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    now_ms: i64,
) -> Result<Plan, Vec<String>> {
    let observation = match watcher.scan(now_ms) {
        Ok(observation) => observation,
        Err(error) => return Err(vec![error_line(&error)]),
    };
    let mut lines: Vec<String> = observation.unresolved.iter().map(error_line).collect();
    let directives = scheduler.on_observation(&observation, now_ms);
    lines.extend(scheduler.unhandled().iter().map(|effect_id| {
        error_line(
            &ExecError::new(
                "exec/unhandled_effect",
                format!("no handler declares {effect_id}'s effect"),
            )
            .details(fsm_core::json::Value::Obj(BTreeMap::from([(
                "effect_id".to_string(),
                fsm_core::json::Value::Str(effect_id.clone()),
            )]))),
        )
    }));
    lines.extend(
        scheduler
            .stalled()
            .iter()
            .map(|effect_id| format!("stalled effect {effect_id}")),
    );
    // A quiet tick that is waiting is not the same as a quiet tick with
    // nothing to do, and an operator watching one should be able to tell.
    lines.extend(
        scheduler
            .deferred()
            .iter()
            .map(|effect_id| format!("waiting to retry {effect_id}")),
    );
    Ok(Plan {
        observation,
        directives,
        lines,
    })
}

/// A run that is over — or never began — and now needs journaling.
struct PendingSettle {
    effect: PendingEffect,
    outcome: RunOutcome,
}

/// Start and stop handlers. This phase never writes, which is what lets a
/// timed-out handler be stopped even on a tick that cannot take the writer.
fn prepare(scheduler: &mut Scheduler, runner: &mut Runner, plan: &mut Plan) -> Vec<PendingSettle> {
    let mut settles = Vec::new();
    // An effect whose argv could not be built never reaches the runner; it is
    // a run that failed before it began, and is acked as one.
    for unstartable in scheduler.unstartable().to_vec() {
        plan.lines.push(error_line(&unstartable.error));
        settles.push(PendingSettle {
            outcome: RunOutcome::NotStarted {
                code: unstartable.error.code,
                detail: unstartable
                    .error
                    .details
                    .as_ref()
                    .and_then(|details| details.get("placeholder"))
                    .and_then(fsm_core::json::Value::as_str)
                    .unwrap_or(unstartable.effect.effect_name.as_str())
                    .to_string(),
            },
            effect: unstartable.effect,
        });
    }
    for directive in &plan.directives {
        match directive {
            Directive::Start { effect, argv, .. } => {
                plan.lines.push(format!(
                    "observed pending {} {}",
                    effect.effect_name, effect.effect_id
                ));
                match runner.spawn(effect.effect_id.clone(), argv) {
                    Ok(()) => plan.lines.push(format!(
                        "spawned handler {} {}",
                        effect.effect_name, effect.effect_id
                    )),
                    Err(error) => {
                        plan.lines.push(error_line(&error));
                        settles.push(PendingSettle {
                            effect: effect.clone(),
                            outcome: RunOutcome::SpawnFailed {
                                argv0: argv.first().cloned().unwrap_or_default(),
                            },
                        });
                    }
                }
            }
            Directive::Kill { effect_id, reason } => {
                let effect = scheduler.inflight_effect(effect_id).cloned().or_else(|| {
                    plan.observation
                        .pending
                        .iter()
                        .find(|effect| &effect.effect_id == effect_id)
                        .cloned()
                });
                let outcome = runner.kill(effect_id, *reason);
                plan.lines
                    .push(format!("killed {} {effect_id}", kill_word(*reason)));
                match effect {
                    Some(effect) => settles.push(PendingSettle { effect, outcome }),
                    None => scheduler.complete(effect_id),
                }
            }
            // The three composition directives reach only the journal, so
            // they never touch the runner: a subprocess is for reaching the
            // world's computers, and creating a child, returning its result,
            // or delivering a signal reaches nothing but this store.
            Directive::SendEvent { .. }
            | Directive::PollDeadline { .. }
            | Directive::InvokeChild { .. }
            | Directive::InvocationReturn { .. }
            | Directive::SignalDeliver { .. } => {}
        }
    }
    settles
}

fn writes_anything(
    directives: &[Directive],
    settles: &[PendingSettle],
    finished: &[String],
) -> bool {
    !settles.is_empty()
        || !finished.is_empty()
        || directives.iter().any(|directive| {
            matches!(
                directive,
                Directive::SendEvent { .. }
                    | Directive::PollDeadline { .. }
                    | Directive::InvokeChild { .. }
                    | Directive::InvocationReturn { .. }
                    | Directive::SignalDeliver { .. }
            )
        })
}

/// Everything that touches the journal, under one writer handle.
#[allow(clippy::too_many_arguments)]
fn settle_phase(
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    store: &mut Store,
    clock: &mut dyn Clock,
    plan: &Plan,
    settles: Vec<PendingSettle>,
    finished: Vec<String>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let pending: BTreeMap<&str, &PendingEffect> = plan
        .observation
        .pending
        .iter()
        .map(|effect| (effect.effect_id.as_str(), effect))
        .collect();

    for pending_settle in settles {
        lines.extend(settle(
            scheduler,
            pipeline,
            store,
            clock,
            &pending_settle.effect,
            pending_settle.outcome,
            plan.observation.to_seq,
        ));
    }

    for directive in &plan.directives {
        match directive {
            Directive::Start { .. } | Directive::Kill { .. } => {}
            Directive::SendEvent {
                instance_id,
                effect_id,
                event,
                payload,
                stamps,
                request_id,
            } => {
                let advance = Advance {
                    event: event.clone(),
                    payload: payload.clone(),
                    stamps: stamps.clone(),
                };
                match pipeline.advance_only(store, clock, effect_id, instance_id, &advance) {
                    Ok(SettleOutcome::Advanced) => {
                        lines.push(format!(
                            "sent {event} {instance_id} request_id={request_id}"
                        ));
                        lines.extend(terminal_line(store, instance_id));
                    }
                    Ok(_) => lines.push(format!("no-advance {effect_id} {event}")),
                    Err(error) => lines.push(error_line(&error)),
                }
            }
            Directive::InvokeChild {
                parent_instance_id,
                slot,
                child_instance_id,
                request_id,
            } => match store.invoke_child_on(clock, parent_instance_id, slot, request_id) {
                Ok(_) => lines.push(format!(
                    "invoked {slot} {parent_instance_id} child={child_instance_id} request_id={request_id}"
                )),
                Err(error) => lines.push(error_line(&ExecError::from_store("exec/invoke", &error))),
            },
            Directive::InvocationReturn {
                parent_instance_id,
                slot,
                child_instance_id,
                request_id,
            } => match store.invocation_return_on(clock, parent_instance_id, slot, request_id) {
                Ok(_) => {
                    lines.push(format!(
                        "returned {slot} {parent_instance_id} child={child_instance_id} request_id={request_id}"
                    ));
                    lines.extend(terminal_line(store, parent_instance_id));
                }
                Err(error) => lines.push(error_line(&ExecError::from_store("exec/invoke", &error))),
            },
            Directive::SignalDeliver {
                sender_instance_id,
                signal_id,
                target_instance_id,
                request_id,
            } => match store.signal_deliver_on(clock, sender_instance_id, signal_id, request_id) {
                Ok(_) => {
                    lines.push(format!(
                        "signalled {sender_instance_id} target={target_instance_id} signal={signal_id} request_id={request_id}"
                    ));
                    lines.extend(terminal_line(store, target_instance_id));
                }
                Err(error) => lines.push(error_line(&ExecError::from_store("exec/signal", &error))),
            },
            Directive::PollDeadline {
                instance_id,
                deadline,
                due_ms,
                request_id,
            } => match pipeline.poll(store, clock, instance_id, deadline, *due_ms) {
                Ok(_) => {
                    // Marked only now: a poll that was decided but never
                    // journaled has to be decided again, or the deadline is
                    // silenced for the life of the process.
                    scheduler.poll_issued(instance_id, deadline, *due_ms);
                    lines.push(format!(
                        "polled deadline {deadline} {instance_id} request_id={request_id}"
                    ));
                    lines.extend(terminal_line(store, instance_id));
                }
                Err(error) => {
                    // A rejected poll journals its rejection and claims the
                    // key, so the journal — not this set — stops the retry.
                    lines.push(error_line(&error));
                }
            },
        }
    }

    for effect_id in finished {
        let Some(outcome) = runner.poll(&effect_id) else {
            continue;
        };
        let effect = scheduler.inflight_effect(&effect_id).cloned().or_else(|| {
            pending
                .get(effect_id.as_str())
                .map(|effect| (*effect).clone())
        });
        match effect {
            Some(effect) => lines.extend(settle(
                scheduler,
                pipeline,
                store,
                clock,
                &effect,
                outcome,
                plan.observation.to_seq,
            )),
            None => scheduler.complete(&effect_id),
        }
    }
    lines
}

/// Ack one outcome and report what the journal now says.
///
/// `scheduler.complete` runs on **every** path out of here — advanced, acked
/// without an advance, already settled, or a store failure. An effect left
/// marked in flight is invisible to the start rule for the life of the
/// process, which is the one way this loop can wedge itself.
fn settle(
    scheduler: &mut Scheduler,
    pipeline: &mut Pipeline,
    store: &mut Store,
    clock: &mut dyn Clock,
    effect: &PendingEffect,
    outcome: RunOutcome,
    parked_at_seq: u64,
) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(handler) = scheduler.handler(&effect.effect_name).cloned() else {
        // The table changed under a restart, or a human acked something this
        // executor has no handler for. Either way there is nothing to write.
        scheduler.complete(&effect.effect_id);
        return vec![format!("unhandled effect {}", effect.effect_id)];
    };
    let acked = if outcome.succeeded() { "ok" } else { "failed" };
    let advance = if outcome.succeeded() {
        handler.on_ok.as_ref()
    } else {
        handler.on_failed.as_ref()
    };
    let event = advance.map(|advance| advance.event.clone());
    match pipeline.settle(store, clock, effect, outcome, &handler) {
        Ok(settled) => {
            if settled != SettleOutcome::AlreadySettled {
                lines.push(format!(
                    "acked {acked} {} request_id={}",
                    effect.effect_id,
                    crate::rid::ack_rid(&effect.effect_id)
                ));
            } else {
                lines.push(format!("already-settled {}", effect.effect_id));
            }
            match (settled, event) {
                (SettleOutcome::Advanced, Some(event)) => {
                    lines.push(format!(
                        "sent {event} {} request_id={}",
                        effect.instance_id,
                        crate::rid::event_rid(&effect.effect_id, &event)
                    ));
                    lines.extend(terminal_line(store, &effect.instance_id));
                }
                (SettleOutcome::AckedNoAdvance, Some(event)) => {
                    scheduler.park_advance(&effect.effect_id, &event, parked_at_seq);
                    lines.push(format!("no-advance {} {event}", effect.effect_id));
                }
                _ => {}
            }
        }
        Err(error) => lines.push(error_line(&error)),
    }
    scheduler.complete(&effect.effect_id);
    lines
}

/// One line when an instance has reached the end of its life, and nothing when
/// it has not.
fn terminal_line(store: &Store, instance_id: &str) -> Option<String> {
    let status = store.state.instances.get(instance_id)?.status;
    match status {
        fsm_core::machine::Status::Running => None,
        other => Some(format!("instance {instance_id} {}", other.as_str())),
    }
}

fn kill_word(reason: KillReason) -> &'static str {
    match reason {
        KillReason::Timeout => "timeout",
        KillReason::Cancelled => "cancelled",
    }
}

/// Errors reach the trace as codes and identifiers, never as messages: a
/// message can carry a path or a temporary directory, and this stream is
/// byte-compared.
fn error_line(error: &ExecError) -> String {
    match (error.code, error.store_code()) {
        (code, Some(store_code)) => format!("error {code} {store_code}"),
        (code, None) => match error
            .details
            .as_ref()
            .and_then(|details| details.get("effect_id"))
            .and_then(fsm_core::json::Value::as_str)
        {
            Some(effect_id) => format!("error {code} {effect_id}"),
            None => format!("error {code}"),
        },
    }
}
