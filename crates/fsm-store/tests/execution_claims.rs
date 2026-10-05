//! Production allocation and persistence; domain values here are structural fixtures.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Barrier;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{Admission, NativeDomain, RetryPolicy};
use fsm_store::clock::FixedClock;
use fsm_store::store::{ExecutionClaimRequest, Store};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fsm-execution-claims-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn domain() -> NativeDomain {
    NativeDomain::from_value(&parse(br#"{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}"#, &JsonLimits::DEFAULT).unwrap()).unwrap()
}

fn policy() -> RetryPolicy {
    RetryPolicy::from_value(
        &parse(
            br#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .unwrap()
}

fn populated(directory: &Directory) -> (Store, String) {
    let mut store = Store::open(&directory.0).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(definition, false, false).unwrap();
    store
        .create_instance("case_review", "instance", "create", None)
        .unwrap();
    store
        .send_event(
            "instance",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send",
            None,
        )
        .unwrap();
    assert_eq!(store.state.execution.admission(), Admission::Enabled);
    let effect = store.state.instances["instance"].pending[0].clone();
    (store, effect)
}

fn allocate(
    store: &mut Store,
    effect: &str,
    request: &str,
    expected: Option<u64>,
) -> Result<Value, Box<fsm_store::store::ErrorObj>> {
    store.claim_execution_on(&mut FixedClock::new(100, 1), ExecutionClaimRequest {
        instance_id: "instance", effect_id: effect,
        handler_fingerprint: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        retry: &policy(), domain: &domain(), request_id: request, expected_seq: expected,
    }).map_err(Box::new)
}

#[test]
fn production_claim_replays_after_reopen_without_allocating_a_successor() {
    let directory = Directory::new();
    let (mut store, effect) = populated(&directory);
    let observed = store.journal.last_seq;
    let first = allocate(&mut store, &effect, "claim", Some(observed)).unwrap();
    let sequence = store.journal.last_seq;
    assert_eq!(store.state.execution.run_high_water(), 1);
    drop(store);
    let mut store = Store::open(&directory.0).unwrap();
    let duplicate = allocate(&mut store, &effect, "claim", Some(observed)).unwrap();
    assert_eq!(duplicate.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(duplicate.get("execution"), first.get("execution"));
    assert_eq!(duplicate.get("seq"), first.get("seq"));
    assert_eq!(store.journal.last_seq, sequence);
    assert_eq!(store.state.execution.run_high_water(), 1);
    assert_eq!(
        allocate(&mut store, &effect, "successor", None)
            .unwrap_err()
            .code,
        "store/execution_owned"
    );
    assert_eq!(store.journal.last_seq, sequence);
    let other = format!("{effect}-other");
    assert_eq!(
        allocate(&mut store, &other, "claim", None)
            .unwrap_err()
            .code,
        "req/request_id_conflict"
    );
}

#[test]
fn stale_or_removed_effect_refuses_before_allocating_or_claiming_a_request() {
    let directory = Directory::new();
    let (mut store, effect) = populated(&directory);
    let head = store.journal.last_seq;
    assert_eq!(
        allocate(&mut store, &effect, "stale", Some(head - 1))
            .unwrap_err()
            .code,
        "store/execution_stale"
    );
    assert_eq!(store.journal.last_seq, head);
    assert!(!store.state.dedup.contains_key("stale"));
    store
        .ack_effect("instance", &effect, "external-ack")
        .unwrap();
    let head = store.journal.last_seq;
    assert_eq!(
        allocate(&mut store, &effect, "removed", None)
            .unwrap_err()
            .code,
        "store/execution_stale"
    );
    assert_eq!(store.state.execution.run_high_water(), 0);
    assert_eq!(store.journal.last_seq, head);
    assert!(!store.state.dedup.contains_key("removed"));
}

#[test]
fn two_racing_production_writers_allocate_exactly_one_claim() {
    let directory = Directory::new();
    let (store, effect) = populated(&directory);
    drop(store);
    let start = Barrier::new(2);
    let attempted = Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|index| {
                let start = &start;
                let attempted = &attempted;
                let path = &directory.0;
                let effect = &effect;
                scope.spawn(move || {
                    start.wait();
                    let mut opened = Store::open(path);
                    let result = match &mut opened {
                        Ok(store) => {
                            allocate(store, effect, &format!("racing-{index}"), None).map(|_| ())
                        }
                        Err(error) => Err(Box::new(error.clone())),
                    };
                    // The winner retains LOCK until both attempts finish.
                    attempted.wait();
                    result
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .next()
            .unwrap()
            .code,
        "store/lock"
    );
    let store = Store::open(&directory.0).unwrap();
    assert_eq!(store.state.execution.run_high_water(), 1);
    assert_eq!(store.state.execution.unresolved().count(), 1);
}

#[test]
fn cancelled_claim_retains_original_hash_across_two_seals_and_reopen() {
    let directory = Directory::new();
    let (mut store, effect) = populated(&directory);
    allocate(&mut store, &effect, "claim", None).unwrap();
    let original = format!("sha256:{}", store.records.last().unwrap().hash);
    store.cancel_instance("instance", "cancel").unwrap();
    assert_eq!(
        store.state.instances["instance"].status,
        fsm_core::machine::Status::Cancelled
    );
    for index in 0..2 {
        let archive = directory.0.join(format!("archive-{index}"));
        std::fs::create_dir_all(&archive).unwrap();
        store.seal_and_archive(&archive, None).unwrap();
        fsm_store::archive::verify(&archive).unwrap();
        drop(store);
        store = Store::open(&directory.0).unwrap();
        let opened = fsm_store::base::open_from_base(&directory.0, &store.records).unwrap();
        assert_eq!(opened.index.execution_claims.get(&1), Some(&original));
        assert_eq!(store.state.execution.run_high_water(), 1);
        assert!(
            store
                .state
                .execution
                .claim_for("instance", &effect)
                .is_some()
        );
    }
}

#[test]
fn read_only_execution_allocation_never_changes_the_journal() {
    let directory = Directory::new();
    let (store, effect) = populated(&directory);
    drop(store);
    let mut store = Store::open_read_only(&directory.0).unwrap();
    let before = store.state.clone();
    assert_eq!(
        allocate(&mut store, &effect, "read-only", None)
            .unwrap_err()
            .code,
        "io/write"
    );
    assert!(fsm_store::snapshot::store_states_eq(&before, &store.state));
    assert!(!store.state.dedup.contains_key("read-only"));
}

#[test]
fn cancellation_between_observation_and_claim_refuses_without_burning_a_run() {
    let directory = Directory::new();
    let (mut store, effect) = populated(&directory);
    store.cancel_instance("instance", "cancel").unwrap();
    let head = store.journal.last_seq;
    assert_eq!(
        allocate(&mut store, &effect, "cancelled", None)
            .unwrap_err()
            .code,
        "store/execution_stale"
    );
    assert_eq!(store.state.execution.run_high_water(), 0);
    assert_eq!(store.journal.last_seq, head);
    assert!(!store.state.dedup.contains_key("cancelled"));
}

#[test]
fn torn_stopped_record_never_releases_durable_claim_ownership() {
    use fsm_core::record::execution::Closure;
    use fsm_core::record::{RecordKind, seal};
    use std::io::Write;
    for boundary in ["prefix", "body", "missing-lf"] {
        let directory = Directory::new();
        let (mut store, effect) = populated(&directory);
        allocate(&mut store, &effect, "claim", None).unwrap();
        let claim = store
            .state
            .execution
            .claim_for("instance", &effect)
            .unwrap()
            .clone();
        let metadata = claim.to_value();
        let head = store.journal.last_seq;
        let hash = store.journal.last_hash.clone();
        // Structural trusted-journal fixture only; no native proof was authenticated.
        let body = Value::Obj(BTreeMap::from([
            ("run_id".into(), Value::Num("1".into())),
            ("instance_id".into(), Value::Str("instance".into())),
            ("effect_id".into(), Value::Str(effect.clone())),
            (
                "handler_fingerprint".into(),
                metadata.get("handler_fingerprint").unwrap().clone(),
            ),
            (
                "closure".into(),
                Closure::new(
                    1,
                    claim.domain().clone(),
                    format!("sha256:{}", "b".repeat(64)),
                )
                .unwrap()
                .to_value(),
            ),
            (
                "outcome".into(),
                Value::Obj(BTreeMap::from([("status".into(), Value::Str("ok".into()))])),
            ),
            ("request_id".into(), Value::Str("torn-stop".into())),
            (
                "request_fp".into(),
                Value::Str(format!("sha256:{}", "c".repeat(64))),
            ),
        ]));
        let bytes = seal(head + 1, 101, RecordKind::ExecutionStopped, body, &hash).to_line();
        let length = match boundary {
            "prefix" => 1,
            "body" => bytes.len() / 2,
            _ => bytes.len() - 1,
        };
        drop(store);
        let segment = directory.0.join("journal/seg-00000000000000000000.jsonl");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment)
            .unwrap();
        file.write_all(&bytes[..length]).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let read_only = Store::open_read_only(&directory.0).unwrap();
        assert_eq!(read_only.state.execution.run_high_water(), 1);
        assert!(
            read_only
                .state
                .execution
                .stopped_for("instance", &effect)
                .is_none()
        );
        assert!(
            read_only
                .state
                .execution
                .claim_for("instance", &effect)
                .is_some()
        );
        drop(read_only);
        assert!(Store::open(&directory.0).is_err());
        let repair = fsm_store::journal_io::repair_truncate_torn_tail(&directory.0).unwrap();
        assert_eq!(repair.truncated_to_seq, head);
        let mut store = Store::open(&directory.0).unwrap();
        assert_eq!(store.journal.last_hash, hash);
        assert_eq!(
            allocate(&mut store, &effect, "successor", None)
                .unwrap_err()
                .code,
            "store/execution_owned"
        );
        assert_eq!(store.journal.last_seq, head);
    }
}

#[test]
fn arbitrary_caller_written_receipts_cannot_construct_opaque_proofs() {
    let directory = Directory::new();
    let receipt = directory.0.join("asserted-proof.json");
    std::fs::write(&receipt, br#"{"format":"fsm.native-closure/1"}"#).unwrap();
    assert_eq!(
        fsm_store::store::VerifiedClosure::read(&receipt)
            .unwrap_err()
            .code,
        "store/execution_evidence"
    );
    assert_eq!(
        fsm_store::store::VerifiedQuiescence::read(&receipt)
            .unwrap_err()
            .code,
        "store/execution_evidence"
    );
}

#[test]
fn historical_attempts_cannot_restart_at_one_after_reopen() {
    let directory = Directory::new();
    let (mut store, effect) = populated(&directory);
    store
        .attempt_effect_on(
            &mut FixedClock::new(100, 1),
            "instance",
            &effect,
            "legacy-failure",
            1,
            None,
        )
        .unwrap();
    let head = store.journal.last_seq;
    drop(store);
    let mut store = Store::open(&directory.0).unwrap();
    assert_eq!(store.attempts_for("instance", &effect), 1);
    assert_eq!(
        allocate(&mut store, &effect, "reset-count", None)
            .unwrap_err()
            .code,
        "store/execution_contract"
    );
    assert_eq!(store.state.execution.run_high_water(), 0);
    assert_eq!(store.journal.last_seq, head);
    assert!(!store.state.dedup.contains_key("reset-count"));
}
