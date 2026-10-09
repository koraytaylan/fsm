//! Count and owned-allocation admission held until the owner finishes a command.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};

use fsm_core::json::Value;

use super::interaction::{Continuation, PrepareFailure};
use super::{
    AdmissionError, Command, Outcome,
    operation::{Operation, ReadOperation},
};
use crate::mcp::cancel::{CancelFlag, Cancellations};

pub(super) enum Reply {
    Outcome(mpsc::SyncSender<Outcome>),
    Prepared(mpsc::SyncSender<Result<Continuation, PrepareFailure>>),
    Retired,
}

pub(super) const HOST_COMMANDS: usize = 32;
pub(super) const HOST_BYTES: usize = 32 * 1024 * 1024;
pub(super) const SESSION_COMMANDS: usize = 8;
pub(super) const SESSION_BYTES: usize = 16 * 1024 * 1024;

pub(super) struct SessionState {
    pub generation: u64,
    open: AtomicBool,
}

impl SessionState {
    pub(super) fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    pub(super) fn close(&self) {
        self.open.store(false, Ordering::Release);
    }
}

#[derive(Default)]
struct Charge {
    commands: usize,
    bytes: usize,
}

#[derive(Default)]
struct State {
    queue: VecDeque<Admitted>,
    host: Charge,
    sessions: BTreeMap<u64, Charge>,
    last_generation: u64,
    last_request: u64,
    controls: BTreeMap<u64, RequestControl>,
    stopped: bool,
}

struct RequestControl {
    generation: u64,
    rpc_id: Value,
    // A bounded host-local token scopes the existing flag implementation;
    // it never allocates or replaces a journal request-id key.
    internal_id: Value,
    cancellations: Cancellations,
}

#[derive(Default)]
pub(super) struct Mailbox {
    state: Mutex<State>,
    ready: Condvar,
}

pub(super) struct Admitted {
    pub session: Arc<SessionState>,
    pub command: Operation,
    pub reply: Reply,
    pub cancel: CancelFlag,
    pub reservation: Reservation,
}

pub(super) struct Reservation {
    mailbox: std::sync::Weak<Mailbox>,
    generation: u64,
    bytes: usize,
    request: u64,
}

impl Reservation {
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Growth is checked atomically against both original charges; count is retained.
    pub(super) fn grow(&mut self, extra: usize) -> Result<(), AdmissionError> {
        let mailbox = self.mailbox.upgrade().ok_or(AdmissionError::Stopped)?;
        let mut state = mailbox.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.stopped {
            return Err(AdmissionError::Stopped);
        }
        if extra > HOST_BYTES.saturating_sub(state.host.bytes)
            || extra
                > SESSION_BYTES.saturating_sub(
                    state
                        .sessions
                        .get(&self.generation)
                        .map_or(0, |charge| charge.bytes),
                )
        {
            return Err(AdmissionError::Busy);
        }
        state.host.bytes += extra;
        state
            .sessions
            .get_mut(&self.generation)
            .expect("original continuation charge")
            .bytes += extra;
        self.bytes += extra;
        Ok(())
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(mailbox) = self.mailbox.upgrade() {
            let mut state = mailbox.state.lock().unwrap_or_else(|p| p.into_inner());
            state.controls.remove(&self.request);
            state.host.commands -= 1;
            state.host.bytes -= self.bytes;
            let charge = state
                .sessions
                .get_mut(&self.generation)
                .expect("admission charge");
            charge.commands -= 1;
            charge.bytes -= self.bytes;
            if charge.commands == 0 {
                state.sessions.remove(&self.generation);
            }
        }
    }
}

impl Mailbox {
    pub(super) fn is_stopped(&self) -> bool {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).stopped
    }

    pub(super) fn session(&self) -> Result<Arc<SessionState>, AdmissionError> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.stopped {
            return Err(AdmissionError::Stopped);
        }
        state.last_generation = state
            .last_generation
            .checked_add(1)
            .ok_or(AdmissionError::GenerationExhausted)?;
        Ok(Arc::new(SessionState {
            generation: state.last_generation,
            open: AtomicBool::new(true),
        }))
    }

    pub(super) fn admit(
        self: &Arc<Self>,
        session: Arc<SessionState>,
        command: Operation,
    ) -> Result<mpsc::Receiver<Outcome>, AdmissionError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.admit_with(session, command, Reply::Outcome(reply))?;
        Ok(receiver)
    }

    pub(super) fn prepare(
        self: &Arc<Self>,
        session: Arc<SessionState>,
        command: Operation,
    ) -> Result<mpsc::Receiver<Result<Continuation, PrepareFailure>>, AdmissionError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.admit_with(session, command, Reply::Prepared(reply))?;
        Ok(receiver)
    }

    fn admit_with(
        self: &Arc<Self>,
        session: Arc<SessionState>,
        command: Operation,
        reply: Reply,
    ) -> Result<(), AdmissionError> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let admitted = self.reserve_locked(&mut state, session, command, reply)?;
        state.queue.push_back(admitted);
        self.ready.notify_one();
        Ok(())
    }

    /// Own bounded admission and controls independently of writer dispatch.
    fn reserve_locked(
        self: &Arc<Self>,
        state: &mut State,
        session: Arc<SessionState>,
        command: Operation,
        reply: Reply,
    ) -> Result<Admitted, AdmissionError> {
        let bytes = operation_charge(&command);
        if state.stopped {
            return Err(AdmissionError::Stopped);
        }
        if !session.is_open() {
            return Err(AdmissionError::Closed);
        }
        let per_session = state.sessions.get(&session.generation);
        if state.host.commands >= HOST_COMMANDS
            || bytes > HOST_BYTES.saturating_sub(state.host.bytes)
            || per_session.is_some_and(|charge| charge.commands >= SESSION_COMMANDS)
            || bytes > SESSION_BYTES.saturating_sub(per_session.map_or(0, |charge| charge.bytes))
        {
            return Err(AdmissionError::Busy);
        }
        let request = state
            .last_request
            .checked_add(1)
            .ok_or(AdmissionError::GenerationExhausted)?;
        let internal_id = Value::Num(request.to_string());
        let cancellations = Cancellations::default();
        let cancel = cancellations.flag(&internal_id);
        state.last_request = request;
        state.controls.insert(
            request,
            RequestControl {
                generation: session.generation,
                rpc_id: command.rpc_id().clone(),
                internal_id,
                cancellations,
            },
        );
        state.host.commands += 1;
        state.host.bytes += bytes;
        let charge = state.sessions.entry(session.generation).or_default();
        charge.commands += 1;
        charge.bytes += bytes;
        let generation = session.generation;
        Ok(Admitted {
            session,
            command,
            reply,
            cancel,
            reservation: Reservation {
                mailbox: Arc::downgrade(self),
                generation,
                bytes,
                request,
            },
        })
    }

    /// A prepared request already owns its slot and original cancellation control.
    pub(super) fn resume(&self, admitted: Admitted) -> Result<(), AdmissionError> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.stopped {
            drop(state);
            return Err(AdmissionError::Stopped);
        }
        if !admitted.session.is_open() {
            drop(state);
            return Err(AdmissionError::Closed);
        }
        state.queue.push_back(admitted);
        self.ready.notify_one();
        Ok(())
    }

    pub(super) fn cancel(&self, generation: u64, rpc_id: &Value) -> usize {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut cancelled = 0;
        for control in state.controls.values_mut() {
            if control.generation == generation && &control.rpc_id == rpc_id {
                control.cancellations.cancel(&control.internal_id);
                cancelled += 1;
            }
        }
        cancelled
    }

    pub(super) fn cancel_generation(&self, generation: u64) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        for control in state.controls.values_mut() {
            if control.generation == generation {
                control.cancellations.cancel(&control.internal_id);
            }
        }
    }

    pub(super) fn next(&self) -> Option<Admitted> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if state.stopped {
                // Reservations acquire state on drop, so retire outside it.
                let rejected = std::mem::take(&mut state.queue);
                drop(state);
                drop(rejected);
                return None;
            }
            if let Some(command) = state.queue.pop_front() {
                return Some(command);
            }
            state = self.ready.wait(state).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Wait on a monotonic deadline without advancing the injected logical clock.
    #[cfg(target_os = "linux")]
    pub(super) fn next_until(&self, deadline: std::time::Instant) -> Next {
        self.next_until_with(deadline, std::time::Instant::now)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn next_until_with(
        &self,
        deadline: std::time::Instant,
        mut now: impl FnMut() -> std::time::Instant,
    ) -> Next {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if state.stopped {
                return Next::Stopped;
            }
            if let Some(command) = state.queue.pop_front() {
                return Next::Command(command);
            }
            let Some(remaining) = deadline.checked_duration_since(now()) else {
                return Next::Due;
            };
            if remaining.is_zero() {
                return Next::Due;
            }
            state = self
                .ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    pub(super) fn stop(&self) {
        let rejected = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.stopped = true;
            std::mem::take(&mut state.queue)
        };
        drop(rejected);
        self.wake();
    }

    pub(super) fn wake(&self) {
        self.ready.notify_all();
    }
}

// Queue slots and admission metadata are count-bounded. Charge the complete
// retained command and capacities, including RPC IDs; there are no wire copies.
// 4096 bytes per object entry conservatively covers BTree node allocation.
pub(super) fn command_charge(command: &Command) -> usize {
    envelope_charge(&command.rpc_id)
        .saturating_add(command.tool.capacity())
        .saturating_add(value_charge(&command.arguments, 0))
}

pub(super) fn operation_charge(operation: &Operation) -> usize {
    match operation {
        Operation::Tool(command) => command_charge(command),
        Operation::HostedTool { command, context } => command_charge(command)
            .saturating_add(std::mem::size_of::<super::operation::HostedToolContext>())
            .saturating_add(context.adapter_bytes)
            .saturating_add(
                context
                    .metadata
                    .as_ref()
                    .map_or(0, |value| value_charge(value, 0).saturating_mul(4)),
            )
            .saturating_add(32 * 1024),
        Operation::Read(command) => {
            envelope_charge(&command.rpc_id).saturating_add(match &command.operation {
                ReadOperation::ResourcesList => 0,
                ReadOperation::ResourceRead { uri } => uri.capacity(),
                ReadOperation::Complete { parameters } => value_charge(parameters, 0),
            })
        }
        Operation::Prepare(command) => envelope_charge(&command.rpc_id)
            .saturating_add(value_charge(&command.arguments, 0))
            .saturating_add(command.adapter_bytes)
            .saturating_add(32 * 1024),
        // Resumption grows the original reservation; it never allocates a new slot.
        Operation::Settle(_) => unreachable!("settlement reuses original admission"),
        Operation::Awaiting { .. } => {
            unreachable!("client-owned continuation is not admitted again")
        }
    }
}

fn envelope_charge(rpc_id: &Value) -> usize {
    std::mem::size_of::<Admitted>()
        .saturating_add(12 * 1024)
        .saturating_add(value_charge(rpc_id, 0))
        .saturating_add(value_charge(rpc_id, 0))
}

pub(super) fn value_charge(value: &Value, depth: usize) -> usize {
    if depth > 32 {
        return usize::MAX;
    }
    let base = std::mem::size_of::<Value>();
    let content = match value {
        Value::Null | Value::Bool(_) => 0,
        Value::Num(text) | Value::Str(text) => text.capacity(),
        Value::Arr(values) => values
            .iter()
            .fold(values.capacity().saturating_mul(base), |bytes, child| {
                bytes.saturating_add(value_charge(child, depth + 1))
            }),
        Value::Obj(fields) => fields.iter().fold(0usize, |bytes, (key, child)| {
            bytes
                .saturating_add(4096)
                .saturating_add(key.capacity())
                .saturating_add(value_charge(child, depth + 1))
        }),
    };
    base.saturating_add(content)
}

#[cfg(target_os = "linux")]
pub(super) enum Next {
    Command(Admitted),
    Due,
    Stopped,
}

/// Retire queued replies on normal return, an unused owner, or unwinding.
#[cfg(target_os = "linux")]
pub(super) struct Retirement(pub Arc<Mailbox>);

#[cfg(target_os = "linux")]
impl Drop for Retirement {
    fn drop(&mut self) {
        self.0.stop();
    }
}
