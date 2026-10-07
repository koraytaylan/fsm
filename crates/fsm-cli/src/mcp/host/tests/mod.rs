//! Independent callers drive the same private owner and admitted envelopes.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::clock::FixedClock;
use crate::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

use super::mailbox::{HOST_COMMANDS, SESSION_BYTES, SESSION_COMMANDS, command_charge};
use super::{AdmissionError, Command, Owner, Publication};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fsm-execution-host-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn command(tool: &str, arguments: &str) -> Command {
    Command {
        rpc_id: Value::Str("rpc-1".into()),
        tool: tool.into(),
        arguments: value(arguments),
    }
}

fn padded_command(bytes: usize) -> Command {
    let mut command = command("machine_list", "{}");
    command.arguments = Value::Str(String::new());
    let base = command_charge(&command);
    command.arguments = Value::Str(String::with_capacity(bytes - base));
    assert_eq!(command_charge(&command), bytes);
    command
}

fn seeded(path: &std::path::Path) -> Store {
    let mut store = Store::open(path).unwrap();
    store.define_machine_on(&mut FixedClock::new(1000, 1), value(r#"{"format":"fsm.machine/1","name":"owner_case","states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"waiting","on":"finish","to":"done"}]}"#), false, false).unwrap();
    store
}

#[test]
fn execution_host_mixed_sessions_capture_one_complete_prefix_and_one_writer() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    assert!(
        Store::open(&scratch.0).is_err(),
        "owner retains the sole writer"
    );
    let first = handle.session().unwrap();
    let second = handle.session().unwrap();
    let created = first
        .submit(command(
            "instance_create",
            r#"{"machine":"owner_case","request_id":"create-1"}"#,
        ))
        .unwrap();
    let observed = second
        .submit(command(
            "instance_get",
            r#"{"instance_id":"inst-create-1"}"#,
        ))
        .unwrap();
    let sent = second.submit(command("instance_send", r#"{"instance_id":"inst-create-1","event":{"name":"finish","payload":{}},"request_id":"finish-1"}"#)).unwrap();
    let terminal = first
        .submit(command(
            "instance_get",
            r#"{"instance_id":"inst-create-1"}"#,
        ))
        .unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let created = created.recv().unwrap();
    let observed = observed.recv().unwrap();
    let sent = sent.recv().unwrap();
    let terminal = terminal.recv().unwrap();
    assert!(created.result.is_ok());
    assert_eq!(
        created.publication,
        Some(Publication {
            first_seq: 2,
            last_seq: 2
        })
    );
    assert_eq!(observed.committed_seq, 2);
    assert!(observed.publication.is_none());
    assert_eq!(
        observed
            .result
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("running")
    );
    assert!(sent.result.is_ok());
    assert_eq!(
        sent.publication,
        Some(Publication {
            first_seq: 3,
            last_seq: 3
        })
    );
    assert_eq!(terminal.committed_seq, 3);
    assert_eq!(
        terminal
            .result
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("completed")
    );
    assert_eq!(created.session_generation, terminal.session_generation);
    assert_ne!(created.session_generation, sent.session_generation);
    handle.stop();
    worker.join().unwrap();
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 3);
}

#[test]
fn execution_host_count_boundaries_and_reserved_stop_do_not_write() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let mut replies = Vec::new();
    for _ in 0..HOST_COMMANDS / SESSION_COMMANDS {
        let session = handle.session().unwrap();
        for _ in 0..SESSION_COMMANDS {
            replies.push(session.submit(command("machine_list", "{}")).unwrap());
        }
        assert!(matches!(
            session.submit(command("machine_list", "{}")),
            Err(AdmissionError::Busy)
        ));
    }
    let extra = handle.session().unwrap();
    assert!(matches!(
        extra.submit(command("machine_list", "{}")),
        Err(AdmissionError::Busy)
    ));
    handle.stop();
    assert!(matches!(
        extra.submit(command("machine_list", "{}")),
        Err(AdmissionError::Stopped)
    ));
    owner.run();
    assert!(replies.into_iter().all(|reply| reply.recv().is_err()));
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 1);
}

#[test]
fn execution_host_exact_owned_byte_limits_hold_through_dequeue() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let first = handle.session().unwrap();
    assert!(matches!(
        first.submit(padded_command(SESSION_BYTES + 1)),
        Err(AdmissionError::Busy)
    ));
    let first_reply = first.submit(padded_command(SESSION_BYTES)).unwrap();
    let second = handle.session().unwrap();
    let second_reply = second.submit(padded_command(SESSION_BYTES)).unwrap();
    let third = handle.session().unwrap();
    assert!(matches!(
        third.submit(command("machine_list", "{}")),
        Err(AdmissionError::Busy)
    ));
    let admitted = owner.mailbox.next().unwrap();
    assert!(
        matches!(
            first.submit(command("machine_list", "{}")),
            Err(AdmissionError::Busy)
        ),
        "in-flight allocations remain charged"
    );
    drop(admitted);
    let third_reply = third.submit(command("machine_list", "{}")).unwrap();
    handle.stop();
    owner.run();
    assert!(first_reply.recv().is_err());
    assert!(second_reply.recv().is_err());
    assert!(third_reply.recv().is_err());
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
}

#[test]
fn execution_host_host_byte_limit_plus_one_is_busy_before_dispatch() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let first = handle.session().unwrap();
    let second = handle.session().unwrap();
    let third = handle.session().unwrap();
    let extra = command("machine_list", "{}");
    let extra_bytes = command_charge(&extra);
    let first_reply = first.submit(padded_command(SESSION_BYTES)).unwrap();
    let second_reply = second
        .submit(padded_command(SESSION_BYTES - extra_bytes + 1))
        .unwrap();
    assert!(matches!(third.submit(extra), Err(AdmissionError::Busy)));
    handle.stop();
    owner.run();
    assert!(first_reply.recv().is_err());
    assert!(second_reply.recv().is_err());
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
}

#[test]
fn execution_host_original_generation_cannot_dispatch_after_close() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let old = handle.session().unwrap();
    let reply = old
        .submit(command(
            "instance_create",
            r#"{"machine":"owner_case","request_id":"old-create"}"#,
        ))
        .unwrap();
    old.close();
    assert!(matches!(
        old.submit(command("machine_list", "{}")),
        Err(AdmissionError::Closed)
    ));
    let replacement = handle.session().unwrap();
    let current = replacement
        .submit(command(
            "instance_create",
            r#"{"machine":"owner_case","request_id":"new-create"}"#,
        ))
        .unwrap();
    let worker = std::thread::spawn(move || owner.run());
    assert!(reply.recv().is_err());
    let outcome = current.recv().unwrap();
    assert_ne!(outcome.session_generation, old.original.generation);
    assert_eq!(outcome.rpc_id, Value::Str("rpc-1".into()));
    assert!(outcome.result.is_ok());
    handle.stop();
    worker.join().unwrap();
    let store = Store::open(&scratch.0).unwrap();
    assert!(!store.state.instances.contains_key("inst-old-create"));
    assert!(store.state.instances.contains_key("inst-new-create"));
}

#[test]
fn execution_host_lost_reply_preserves_replay_and_content_conflict() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let session = handle.session().unwrap();
    let args = r#"{"machine":"owner_case","request_id":"create-1"}"#;
    drop(session.submit(command("instance_create", args)).unwrap());
    let replay = session.submit(command("instance_create", args)).unwrap();
    let conflict = session
        .submit(command(
            "instance_create",
            r#"{"machine":"owner_case","request_id":"create-1","tags":["different"]}"#,
        ))
        .unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let replay = replay.recv().unwrap();
    assert!(replay.result.is_ok());
    assert_eq!(replay.committed_seq, 2);
    assert!(replay.publication.is_none());
    let conflict = conflict.recv().unwrap();
    assert_eq!(conflict.result.unwrap_err().code, "req/request_id_conflict");
    assert_eq!(conflict.committed_seq, 2);
    assert!(conflict.publication.is_none());
    handle.stop();
    worker.join().unwrap();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}

#[test]
fn execution_host_concurrent_callers_observe_their_complete_writes_in_fifo_order() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let worker = std::thread::spawn(move || owner.run());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let mut callers = Vec::new();
    for number in 0..8 {
        let session = handle.session().unwrap();
        let barrier = std::sync::Arc::clone(&barrier);
        callers.push(std::thread::spawn(move || {
            barrier.wait();
            let created = session
                .submit(command(
                    "instance_create",
                    &format!(r#"{{"machine":"owner_case","request_id":"concurrent-{number}"}}"#),
                ))
                .unwrap();
            let observed = session
                .submit(command(
                    "instance_get",
                    &format!(r#"{{"instance_id":"inst-concurrent-{number}"}}"#),
                ))
                .unwrap();
            let created = created.recv().unwrap();
            let observed = observed.recv().unwrap();
            assert!(created.result.is_ok());
            assert_eq!(
                created.publication,
                Some(Publication {
                    first_seq: created.committed_seq,
                    last_seq: created.committed_seq,
                })
            );
            assert!(observed.committed_seq >= created.committed_seq);
            assert!(observed.publication.is_none());
            assert_eq!(
                observed
                    .result
                    .unwrap()
                    .get("instance_id")
                    .and_then(Value::as_str),
                Some(format!("inst-concurrent-{number}").as_str())
            );
            created.committed_seq
        }));
    }
    let sequences: std::collections::BTreeSet<_> = callers
        .into_iter()
        .map(|caller| caller.join().unwrap())
        .collect();
    assert_eq!(sequences, (2..=9).collect());
    assert!(Store::open(&scratch.0).is_err());
    handle.stop();
    worker.join().unwrap();
    let store = Store::open(&scratch.0).unwrap();
    assert_eq!(store.journal.last_seq, 9);
    assert_eq!(store.state.instances.len(), 8);
}
