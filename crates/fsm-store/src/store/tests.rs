use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Per-process counter. Tests in one binary run concurrently and a
/// timestamp alone collides: two threads landing in the same nanosecond
/// bucket share a directory, and one wipes the other's store mid-run. It
/// showed up first on a fast macOS release build.
static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A scratch directory that removes itself.
///
/// Every temp directory a test makes has to be given back: a suite that
/// leaks one per run exhausts a long-lived machine's tmpfs inodes long
/// before it exhausts its bytes, and the failure looks like a broken
/// toolchain rather than a leaky test.
struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp() -> Scratch {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let i = TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("fsm-s-{pid}-{n}-{i}"));
    fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn case_def() -> Value {
    parse(
        include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn timed_parallel_def() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1","name":"timed_parallel",
            "context":[{"name":"fires","ty":"int","init":"0"}],
            "events":[{"name":"finish","fields":[]}],
            "regions":[
                {"name":"timer","states":[{"name":"waiting"},{"name":"expired","terminal":true}],"initial":"waiting"},
                {"name":"work","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working"}
            ],
            "transitions":[{"from":"working","on":"finish","to":"done"}],
            "deadlines":[{"name":"expire","from":"waiting","after":"dur(10, ms)","to":"expired","do":[{"target":"fires","value":"ctx.fires + 1"}]}]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn define_idempotent_and_resolve() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    let d1 = s.define_machine(case_def(), false, false).unwrap();
    assert!(d1.created);
    let n = s.journal.last_seq;
    let d2 = s.define_machine(case_def(), false, false).unwrap();
    assert!(!d2.created);
    assert_eq!(d1.machine_id, d2.machine_id);
    assert_eq!(s.journal.last_seq, n);
    s.resolve_machine(&d1.machine_id).unwrap();
    s.resolve_machine("case_review").unwrap();
    let pref = format!(
        "case_review@sha256:{}",
        &d1.machine_id.split(':').next_back().unwrap()[..12]
    );
    s.resolve_machine(&pref).unwrap();
    assert!(dir.join("VERSION").exists());
    assert!(dir.join("journal").exists());
}

#[test]
fn lost_response_retry_returns_original() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case_def(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    let seq = s.journal.last_seq;
    let r1 = s
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", Some(seq))
        .unwrap();
    let n = s.journal.last_seq;
    let r2 = s
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", Some(seq))
        .unwrap();
    assert_eq!(s.journal.last_seq, n);
    assert_eq!(r2.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(r1.get("leaf"), r2.get("leaf"));
    assert_eq!(r1.get("configuration"), r2.get("configuration"));
    assert_eq!(r1.get("context"), r2.get("context"));
    assert_eq!(r1.get("effects_pending"), r2.get("effects_pending"));
    assert_eq!(r1.get("enabled_events"), r2.get("enabled_events"));
    assert_eq!(r1.get("state_hash"), r2.get("state_hash"));
}

#[test]
fn deadline_poll_is_durable_idempotent_and_parallel_safe() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    let mut define_clock = crate::clock::FixedClock::new(1, 1);
    store
        .define_machine_on(&mut define_clock, timed_parallel_def(), false, false)
        .unwrap();
    let mut create_clock = crate::clock::FixedClock::new(100, 1);
    store
        .create_instance_ctx_on(
            &mut create_clock,
            "timed_parallel",
            "timed-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    assert_eq!(create_clock.now, 101, "creation reads its clock once");
    assert_eq!(
        store
            .state
            .instances
            .get("timed-1")
            .unwrap()
            .deadlines
            .get("expire"),
        Some(&110)
    );

    let mut snapshot_clock = crate::clock::FixedClock::new(101, 1);
    store.shutdown_snapshot_on(&mut snapshot_clock).unwrap();
    drop(store);
    let mut store = Store::open(&dir).unwrap();
    assert_eq!(
        store
            .state
            .instances
            .get("timed-1")
            .unwrap()
            .deadlines
            .get("expire"),
        Some(&110)
    );

    let mut early_clock = crate::clock::FixedClock::new(109, 1);
    let early = store
        .poll_instance_deadline_on(&mut early_clock, "timed-1", "poll-early", None)
        .unwrap();
    assert_eq!(early_clock.now, 110, "poll reads its clock once");
    assert_eq!(
        early.get("deadline_not_due").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        early.get("next_deadline").and_then(Value::as_str),
        Some("expire")
    );
    assert_eq!(
        early.get("next_due_ms").and_then(Value::as_str),
        Some("110")
    );
    let early_seq = store.journal.last_seq;

    let mut retry_clock = crate::clock::FixedClock::new(999, 1);
    let retry = store
        .poll_instance_deadline_on(&mut retry_clock, "timed-1", "poll-early", Some(0))
        .unwrap();
    assert_eq!(retry_clock.now, 999, "dedup precedes the clock read");
    assert_eq!(store.journal.last_seq, early_seq);
    assert_eq!(retry.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(retry.get("next_deadline"), early.get("next_deadline"));
    assert_eq!(
        retry.get("next_deadline_idx"),
        early.get("next_deadline_idx")
    );
    assert_eq!(retry.get("next_due_ms"), early.get("next_due_ms"));

    let mut due_clock = crate::clock::FixedClock::new(110, 1);
    let fired = store
        .poll_instance_deadline_on(&mut due_clock, "timed-1", "poll-due", None)
        .unwrap();
    assert_eq!(due_clock.now, 111);
    assert_eq!(
        fired.get("deadline_applied").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fired.get("deadline").and_then(Value::as_str),
        Some("expire")
    );
    let configuration = fired.get("configuration").and_then(Value::as_obj).unwrap();
    assert_eq!(
        configuration.get("kind").and_then(Value::as_str),
        Some("parallel")
    );
    assert_eq!(
        fired
            .get("context")
            .and_then(Value::as_obj)
            .and_then(|context| context.get("fires"))
            .and_then(Value::as_str),
        Some("1")
    );
    assert_eq!(
        store.records.last().map(|record| record.kind),
        Some(RecordKind::DeadlineApplied)
    );

    let fired_seq = store.journal.last_seq;
    drop(store);
    let mut reopened = Store::open(&dir).unwrap();
    let mut lost_response_clock = crate::clock::FixedClock::new(5_000, 1);
    let duplicate = reopened
        .poll_instance_deadline_on(&mut lost_response_clock, "timed-1", "poll-due", None)
        .unwrap();
    assert_eq!(lost_response_clock.now, 5_000);
    assert_eq!(reopened.journal.last_seq, fired_seq);
    assert_eq!(
        duplicate.get("deadline_applied").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        duplicate.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );

    let mut finish_payload = Value::Obj(BTreeMap::new());
    let mut finish_clock = crate::clock::FixedClock::new(111, 1);
    let completed = reopened
        .send_event_stamp_on(
            &mut finish_clock,
            "timed-1",
            "finish",
            &mut finish_payload,
            "finish-1",
            None,
            &[],
        )
        .unwrap();
    assert_eq!(
        completed.get("status").and_then(Value::as_str),
        Some("completed")
    );
    assert!(
        reopened
            .state
            .instances
            .get("timed-1")
            .unwrap()
            .deadlines
            .is_empty()
    );
}

#[test]
fn cancelled_deadline_poll_rejection_is_durable() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store
        .define_machine(timed_parallel_def(), false, false)
        .unwrap();
    store
        .create_instance("timed_parallel", "timed-2", "create-2", None)
        .unwrap();
    store
        .cancel_instance_reason("timed-2", "cancel-2", "operator")
        .unwrap();
    assert!(
        store
            .state
            .instances
            .get("timed-2")
            .unwrap()
            .deadlines
            .is_empty()
    );
    let mut clock = crate::clock::FixedClock::new(1_000, 1);
    let error = store
        .poll_instance_deadline_on(&mut clock, "timed-2", "poll-cancelled", None)
        .unwrap_err();
    assert_eq!(error.code, "run/instance_cancelled");
    assert_eq!(
        store.records.last().map(|record| record.kind),
        Some(RecordKind::RequestRejected)
    );
    drop(store);
    let mut reopened = Store::open(&dir).unwrap();
    let mut retry_clock = crate::clock::FixedClock::new(2_000, 1);
    let duplicate = reopened
        .poll_instance_deadline_on(&mut retry_clock, "timed-2", "poll-cancelled", None)
        .unwrap_err();
    assert_eq!(duplicate, error.mark_duplicate());
    assert_eq!(retry_clock.now, 2_000);
}

fn strip_dup(v: &Value) -> Value {
    let mut c = v.clone();
    if let Value::Obj(o) = &mut c {
        o.remove("duplicate");
    }
    c
}

#[test]
fn reopen_retry_matches_original_bytes() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case_def(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    let r1 = s
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    let _ = s
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "S", None)
        .unwrap();
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let r2 = s2
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    assert_eq!(r2.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(strip_dup(&r1), strip_dup(&r2));
    assert_eq!(
        r2.get("state_path")
            .and_then(Value::as_arr)
            .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>()),
        Some(vec!["in_review", "docs_review"])
    );
}

#[test]
fn allocator_skips_explicit_ids() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case_def(), false, false).unwrap();
    let before = s.journal.last_seq;
    let taken = format!("req-{}-{}", before + 1, before + 2);
    s.create_instance("case_review", "i1", &taken, None)
        .unwrap();
    assert_eq!(s.journal.last_seq, before + 1);
    let next = s.allocate_request_id().unwrap();
    assert_ne!(next, taken);
    assert_eq!(next, format!("req-{}-{}", before + 1, before + 3));
    fs::write(dir.join("alloc"), "").unwrap();
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let after_torn = s2.allocate_request_id().unwrap();
    assert!(!s2.state.dedup.contains_key(&after_torn));
    assert_ne!(after_torn, taken);
    assert!(after_torn.starts_with("req-"));
}

#[test]
fn ack_and_annotate_retry_keep_shape() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case_def(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    let eid = s
        .state
        .instances
        .get("i1")
        .unwrap()
        .pending
        .first()
        .cloned()
        .unwrap();
    let a1 = s
        .ack_effect_outcome("i1", &eid, "ack1", "ok", None)
        .unwrap();
    let n1 = s.annotate("i1", "n1", "hello").unwrap();
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let a2 = s2
        .ack_effect_outcome("i1", &eid, "ack1", "ok", None)
        .unwrap();
    let n2 = s2.annotate("i1", "n1", "hello").unwrap();
    assert_eq!(
        a2.get("effect_id").and_then(Value::as_str),
        Some(eid.as_str())
    );
    assert_eq!(a2.get("acked").and_then(Value::as_bool), Some(true));
    assert_eq!(strip_dup(&a1), strip_dup(&a2));
    assert_eq!(n2.get("note").and_then(Value::as_str), Some("hello"));
    assert_eq!(strip_dup(&n1), strip_dup(&n2));
}

#[test]
fn seq_mismatch_not_consumed() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case_def(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    let n = s.journal.last_seq;
    let err = s
        .send_event(
            "i1",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "fresh",
            Some(0),
        )
        .unwrap_err();
    assert_eq!(err.code, "req/seq_mismatch");
    assert_eq!(s.journal.last_seq, n);
    s.send_event(
        "i1",
        "docs_ok",
        Value::Obj(BTreeMap::new()),
        "fresh",
        Some(n),
    )
    .unwrap();
}

#[test]
fn rejected_retry_keeps_request_id() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case_def(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    let e1 = s
        .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    assert_eq!(
        e1.details.get("request_id").and_then(Value::as_str),
        Some("r")
    );
    let pending = s.state.instances.get("i1").unwrap().pending.clone();
    let ae1 = s
        .ack_effect_outcome("i1", "nope", "ar", "ok", None)
        .unwrap_err();
    assert_eq!(
        ae1.details.get("request_id").and_then(Value::as_str),
        Some("ar")
    );
    assert!(!pending.is_empty());
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let e2 = s2
        .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    assert!(!e1.duplicate);
    assert!(e2.duplicate);
    let mut e1b = e1.clone();
    let mut e2b = e2.clone();
    e1b.duplicate = false;
    e2b.duplicate = false;
    assert_eq!(e1b.to_value(), e2b.to_value());
    let ae2 = s2
        .ack_effect_outcome("i1", "nope", "ar", "ok", None)
        .unwrap_err();
    assert!(!ae1.duplicate);
    assert!(ae2.duplicate);
    let mut a1b = ae1.clone();
    let mut a2b = ae2.clone();
    a1b.duplicate = false;
    a2b.duplicate = false;
    assert_eq!(a1b.to_value(), a2b.to_value());
}
