//! Original continuation identity, accounting and dedup through the owner.

use std::{collections::BTreeMap, time::Duration};

use fsm_core::json::Value;

use crate::{clock::FixedClock, store::Store};

use super::{Scratch, command, value};
use crate::mcp::host::{
    AdmissionError, Owner, Session,
    interaction::Continuation,
    mailbox::{HOST_BYTES, SESSION_BYTES, SESSION_COMMANDS},
    operation::PrepareCommand,
};

pub(super) const CASE: &str = r#"{"format":"fsm.machine/1","name":"question_case","context":[],"events":[{"name":"decide","fields":[{"name":"score","ty":"int"}]}],"effects":[],"states":[{"name":"waiting"},{"name":"ready"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[{"from":"waiting","on":"decide","to":"done"},{"from":"ready","on":"decide","to":"done"}],"deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"ready"}]}"#;

fn seeded(path: &std::path::Path) -> Store {
    let mut store = Store::open(path).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "inst-question",
            "question-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn preparation(rpc_id: &str) -> PrepareCommand {
    PrepareCommand {
        rpc_id: Value::Str(rpc_id.into()),
        arguments: value(
            r#"{"instance_id":"inst-question","event":"decide","request_id":"settle-once"}"#,
        ),
        client_elicitation: true,
        adapter_bytes: 0,
    }
}

fn prepare_one(owner: &mut Owner<FixedClock>, session: &Session, rpc_id: &str) -> Continuation {
    let response = session.prepare(preparation(rpc_id)).unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    response
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap()
}

fn capacity_answer(total: usize, continuation: &Continuation) -> Value {
    let capacity = total - continuation.charged_bytes() - std::mem::size_of::<Value>();
    let text = String::with_capacity(capacity);
    assert_eq!(text.capacity(), capacity);
    Value::Str(text)
}

#[test]
fn execution_host_session_channels_continuation_resumes_at_full_count_and_replays_before_sequence()
{
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let mut pending = (0..SESSION_COMMANDS)
        .map(|index| prepare_one(&mut owner, &session, &format!("question-{index}")))
        .collect::<Vec<_>>();
    assert_eq!(
        session.submit(command("machine_list", "{}")).err(),
        Some(AdmissionError::Busy)
    );
    let first = pending.remove(0);
    let response = session
        .resume(first, value(r#"{"action":"accept","content":{"score":7}}"#))
        .unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    let first = response.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(first.rpc_id, Value::Str("question-0".into()));
    assert_eq!(first.session_generation, session.original.generation);
    assert_eq!(first.committed_seq, 3);
    assert!(first.result.is_ok());
    let replay = pending.remove(0);
    let response = session
        .resume(
            replay,
            value(r#"{"action":"accept","content":{"score":7}}"#),
        )
        .unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    let replay = response.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(replay.rpc_id, Value::Str("question-1".into()));
    assert_eq!(
        replay.result.unwrap().get("duplicate"),
        Some(&Value::Bool(true))
    );
    assert_eq!(replay.committed_seq, 3);
    assert!(replay.publication.is_none());
    drop(pending);
    assert_eq!(session.cancel(&Value::Str("question-7".into())), 0);
    handle.stop();
    owner.run();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 3);
}

#[test]
fn execution_host_session_channels_other_generation_cannot_resume_the_original_question() {
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let original = handle.session().unwrap();
    let replacement = handle.session().unwrap();
    let continuation = prepare_one(&mut owner, &original, "shared-rpc");
    assert_eq!(
        replacement
            .resume(
                continuation,
                value(r#"{"action":"accept","content":{"score":7}}"#)
            )
            .err(),
        Some(AdmissionError::Closed)
    );
    assert_eq!(original.cancel(&Value::Str("shared-rpc".into())), 0);
    let response = original.submit(command("instance_send", r#"{"instance_id":"inst-question","event":{"name":"decide","payload":{"score":"7"}},"request_id":"settle-once"}"#)).unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    assert!(
        response
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .result
            .is_ok()
    );
    handle.stop();
    owner.run();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 3);
}

#[test]
fn execution_host_session_channels_unrelated_commit_does_not_stale_the_target_question() {
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let first = handle.session().unwrap();
    let other = handle.session().unwrap();
    let continuation = prepare_one(&mut owner, &first, "unchanged-target");
    let response = other
        .submit(command(
            "instance_create",
            r#"{"machine":"question_case","request_id":"unrelated-create"}"#,
        ))
        .unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    assert!(
        response
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .result
            .is_ok()
    );
    assert_eq!(owner.store.journal.last_seq, 3);
    let response = first
        .resume(
            continuation,
            value(r#"{"action":"accept","content":{"score":7}}"#),
        )
        .unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    let outcome = response.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(outcome.result.is_ok());
    assert_eq!(outcome.committed_seq, 4);
    handle.stop();
    owner.run();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 4);
}

#[test]
fn execution_host_session_channels_cancel_reaches_a_waiting_question_at_full_count() {
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let mut pending = (0..SESSION_COMMANDS)
        .map(|index| prepare_one(&mut owner, &session, &format!("cancel-{index}")))
        .collect::<Vec<_>>();
    assert_eq!(
        session.submit(command("machine_list", "{}")).err(),
        Some(AdmissionError::Busy)
    );
    assert_eq!(session.cancel(&Value::Str("cancel-0".into())), 1);
    let continuation = pending.remove(0);
    assert!(continuation.cancellation().cancelled());
    let response = session
        .resume(
            continuation,
            value(r#"{"action":"accept","content":{"score":7}}"#),
        )
        .unwrap();
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    assert_eq!(
        response
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .result
            .unwrap_err()
            .code,
        "req/cancelled"
    );
    assert_eq!(owner.store.journal.last_seq, 2);
    assert!(!owner.store.state.dedup.contains_key("settle-once"));
    drop(pending);
    handle.stop();
    owner.run();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}

#[test]
fn execution_host_session_channels_answer_growth_accepts_exact_session_bytes_and_refuses_one_more()
{
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let exact = prepare_one(&mut owner, &session, "exact-answer");
    let answer = capacity_answer(SESSION_BYTES, &exact);
    let response = session.resume(exact, answer).unwrap();
    assert_eq!(
        session.submit(command("machine_list", "{}")).err(),
        Some(AdmissionError::Busy)
    );
    let admitted = owner.mailbox.next().unwrap();
    owner.apply(admitted);
    let outcome = response.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        outcome.result.unwrap().get("applied"),
        Some(&Value::Bool(false))
    );
    assert_eq!(outcome.committed_seq, 2);
    let over = prepare_one(&mut owner, &session, "over-answer");
    let answer = capacity_answer(SESSION_BYTES + 1, &over);
    assert_eq!(
        session.resume(over, answer).err(),
        Some(AdmissionError::Busy)
    );
    assert_eq!(session.cancel(&Value::Str("over-answer".into())), 0);
    assert_eq!(owner.store.journal.last_seq, 2);
    assert!(!owner.store.state.dedup.contains_key("settle-once"));
    handle.stop();
    owner.run();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}

#[test]
fn execution_host_session_channels_answer_growth_accepts_exact_host_bytes_and_refuses_one_more() {
    for over in [false, true] {
        let scratch = Scratch::new();
        let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
        let sessions = (0..3)
            .map(|_| handle.session().unwrap())
            .collect::<Vec<_>>();
        let first = prepare_one(&mut owner, &sessions[0], "host-first");
        let second = prepare_one(&mut owner, &sessions[1], "host-second");
        let third = prepare_one(&mut owner, &sessions[2], "host-third");
        let answer = capacity_answer(SESSION_BYTES, &first);
        let _first = sessions[0].resume(first, answer).unwrap();
        let second_total =
            HOST_BYTES - SESSION_BYTES - third.charged_bytes() - std::mem::size_of::<Value>()
                + usize::from(over);
        let answer = capacity_answer(second_total, &second);
        let _second = sessions[1].resume(second, answer).unwrap();
        let response = sessions[2].resume(third, Value::Null);
        if over {
            assert_eq!(response.err(), Some(AdmissionError::Busy));
        } else {
            assert!(response.is_ok());
        }
        assert_eq!(owner.store.journal.last_seq, 2);
        assert!(!owner.store.state.dedup.contains_key("settle-once"));
        // These actual queued settlements are rejected without processing.
        handle.stop();
        owner.run();
        assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
    }
}

#[test]
fn execution_host_session_channels_borrowed_would_block_returns_to_the_legacy_timeout_check() {
    use crate::mcp::notify::{Notifier, SessionIo, SharedSink};
    use std::io::{self, BufRead, Read};
    struct IdleOnce(bool);
    impl Read for IdleOnce {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }
    impl BufRead for IdleOnce {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if self.0 {
                self.0 = false;
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "legacy HTTP input idle",
                ))
            } else {
                Ok(&[])
            }
        }
        fn consume(&mut self, _amount: usize) {}
    }
    let sink = SharedSink::new();
    let output = Notifier::new(Box::new(sink.writer()));
    let mut input = IdleOnce(true);
    let mut session = SessionIo::new(&output, &mut input);
    let error = crate::mcp::elicit::request_and_await(
        &mut session,
        "elicitation/create",
        Value::Null,
        &mut FixedClock::new(1000, crate::mcp::elicit::DEFAULT_TIMEOUT_MS),
    )
    .unwrap_err();
    assert_eq!(error.code, "req/elicit_timeout");
    assert_eq!(
        error.message,
        format!(
            "no answer within {} ms",
            crate::mcp::elicit::DEFAULT_TIMEOUT_MS
        )
    );
}
