//! Production allocation and persistence; domain values here are structural fixtures.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Barrier;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::canon::canon_bytes;
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
    populated_named(directory, "instance")
}

fn populated_named(directory: &Directory, instance_id: &str) -> (Store, String) {
    let mut store = Store::open(&directory.0).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(definition, false, false).unwrap();
    store
        .create_instance("case_review", instance_id, "create", None)
        .unwrap();
    store
        .send_event(
            instance_id,
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send",
            None,
        )
        .unwrap();
    assert_eq!(store.state.execution.admission(), Admission::Enabled);
    let effect = store.state.instances[instance_id].pending[0].clone();
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
fn claim_append_boundaries_recover_only_the_complete_durable_prefix() {
    // Reproduce interrupted append bytes from a production-generated record;
    // no native allocation or launch occurs before this durability boundary.
    for boundary in ["empty", "prefix", "body", "missing-lf", "complete"] {
        let directory = Directory::new();
        let (mut store, effect) = populated(&directory);
        let head = store.journal.last_seq;
        let hash = store.journal.last_hash.clone();
        let segment = directory.0.join("journal/seg-00000000000000000000.jsonl");
        let prefix = std::fs::read(&segment).unwrap();
        allocate(&mut store, &effect, "claim", None).unwrap();
        let complete = std::fs::read(&segment).unwrap();
        assert!(complete.starts_with(&prefix));
        let record_bytes = &complete[prefix.len()..];
        let keep = match boundary {
            "empty" => 0,
            "prefix" => 1,
            "body" => record_bytes.len() / 2,
            "missing-lf" => record_bytes.len() - 1,
            _ => record_bytes.len(),
        };
        drop(store);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&segment)
            .unwrap();
        file.set_len((prefix.len() + keep) as u64).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let read_only = Store::open_read_only(&directory.0).unwrap();
        let durable = boundary == "complete";
        assert_eq!(
            read_only.state.execution.run_high_water(),
            u64::from(durable)
        );
        assert_eq!(read_only.state.dedup.contains_key("claim"), durable);
        assert_eq!(
            std::fs::read(&segment).unwrap(),
            complete[..prefix.len() + keep]
        );
        drop(read_only);
        if keep > 0 && !durable {
            assert!(Store::open(&directory.0).is_err());
            let repair = fsm_store::journal_io::repair_truncate_torn_tail(&directory.0).unwrap();
            assert_eq!(repair.truncated_to_seq, head);
            assert_eq!(std::fs::read(&segment).unwrap(), prefix);
        }
        let mut store = Store::open(&directory.0).unwrap();
        if durable {
            assert_eq!(store.journal.last_seq, head + 1);
            assert_eq!(
                allocate(&mut store, &effect, "successor", None)
                    .unwrap_err()
                    .code,
                "store/execution_owned"
            );
            assert_eq!(store.state.execution.run_high_water(), 1);
        } else {
            assert_eq!(store.journal.last_seq, head);
            assert_eq!(store.journal.last_hash, hash);
            assert_eq!(store.state.execution.run_high_water(), 0);
            allocate(&mut store, &effect, "claim", None).unwrap();
            assert_eq!(store.state.execution.run_high_water(), 1);
        }
    }
}

#[test]
fn historical_attempts_cannot_restart_after_cache_cold_replay_or_sealing() {
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
    // SPEC execution admission preserves unbound historical failed counts,
    // including when a committed checkpoint selects a disposable cache.
    while store.journal.last_seq < 10_000 {
        let request = format!("legacy-checkpoint-{}", store.journal.last_seq);
        store.annotate("instance", &request, "").unwrap();
    }
    let head = store.journal.last_seq;
    drop(store);
    let mut store = Store::open(&directory.0).unwrap();
    assert!(store.opened_from_snapshot);
    assert_eq!(store.journal.last_seq, head);
    for phase in ["cache", "cold", "first-seal", "second-seal"] {
        if phase != "cache" {
            if phase != "cold" {
                let archive = directory.0.join(phase);
                std::fs::create_dir_all(&archive).unwrap();
                store.seal_and_archive(&archive, None).unwrap();
                fsm_store::archive::verify(&archive).unwrap();
            }
            drop(store);
            std::fs::remove_dir_all(directory.0.join("snapshots")).unwrap();
            store = Store::open(&directory.0).unwrap();
            assert!(!store.opened_from_snapshot);
        }
        let head = store.journal.last_seq;
        let hash = store.journal.last_hash.clone();
        let request = format!("reset-count-{phase}");
        assert_eq!(store.attempts_for("instance", &effect), 1);
        assert_eq!(
            allocate(&mut store, &effect, &request, None)
                .unwrap_err()
                .code,
            "store/execution_contract"
        );
        assert_eq!(store.state.execution.run_high_water(), 0);
        assert_eq!(store.journal.last_seq, head);
        assert_eq!(store.journal.last_hash, hash);
        assert!(!store.state.dedup.contains_key(&request));
        assert!(matches!(
            fsm_store::journal_io::verify(&directory.0).health,
            fsm_store::journal_io::JournalHealth::Ok
        ));
    }
}

#[test]
fn loaders_refuse_semantically_invalid_claims_even_with_a_valid_chain_hash() {
    use fsm_core::record::seal;
    for (field, replacement) in [
        ("run_id", Value::Num("2".into())),
        ("attempt", Value::Num("2".into())),
        ("effect_id", Value::Str("not-pending".into())),
        ("instance_id", Value::Str("unknown-instance".into())),
    ] {
        let directory = Directory::new();
        let (mut store, effect) = populated(&directory);
        let segment = directory.0.join("journal/seg-00000000000000000000.jsonl");
        let mut bytes = std::fs::read(&segment).unwrap();
        allocate(&mut store, &effect, "claim", None).unwrap();
        let record = store.records.last().unwrap();
        let Value::Obj(mut body) = record.body.clone() else {
            panic!("production claim body must be an object");
        };
        body.insert(field.into(), replacement);
        // Recompute the chain hash to isolate semantic folding from hash checks.
        bytes.extend(
            seal(
                record.seq,
                record.ts,
                record.kind,
                Value::Obj(body),
                &record.prev,
            )
            .to_line(),
        );
        drop(store);
        std::fs::write(&segment, &bytes).unwrap();
        assert!(Store::open_read_only(&directory.0).is_err(), "{field}");
        assert_eq!(std::fs::read(&segment).unwrap(), bytes);
        assert!(Store::open(&directory.0).is_err(), "{field}");
        assert_eq!(std::fs::read(&segment).unwrap(), bytes);
    }
}

#[test]
fn claim_at_the_root_checkpoint_survives_bound_snapshot_and_cold_replay() {
    let directory = Directory::new();
    let (mut store, effect) = populated(&directory);
    while store.journal.last_seq < 9_999 {
        let request = format!("checkpoint-padding-{}", store.journal.last_seq);
        store.annotate("instance", &request, "").unwrap();
    }
    let first = allocate(&mut store, &effect, "checkpoint-claim", Some(9_999)).unwrap();
    let record = store.records.last().unwrap();
    assert_eq!(record.seq, 10_000);
    assert_eq!(
        record.body.get("state_root_format").and_then(Value::as_str),
        Some("fsm.state-root/4")
    );
    assert_eq!(
        record.body.get("state_root").and_then(Value::as_str),
        Some(fsm_core::replay::state_root_at(&store.state, 10_000).as_str())
    );
    let claim_hash = record.hash.clone();
    drop(store);
    assert!(matches!(
        fsm_store::journal_io::verify(&directory.0).health,
        fsm_store::journal_io::JournalHealth::Ok
    ));
    let mut store = Store::open(&directory.0).unwrap();
    assert!(
        store.opened_from_snapshot,
        "the committed root must bind the claim snapshot"
    );
    assert_eq!(store.journal.last_seq, 10_000);
    assert_eq!(store.journal.last_hash, claim_hash);
    assert_eq!(store.state.execution.run_high_water(), 1);
    let replay = allocate(&mut store, &effect, "checkpoint-claim", Some(9_999)).unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(replay.get("execution"), first.get("execution"));
    assert_eq!(
        allocate(&mut store, &effect, "successor", None)
            .unwrap_err()
            .code,
        "store/execution_owned"
    );
    assert_eq!(store.journal.last_seq, 10_000);
}

#[test]
fn production_request_id_limit_counts_utf8_bytes_without_allocating_on_refusal() {
    for request in ["r".repeat(4096), "é".repeat(2048)] {
        assert_eq!(request.len(), 4096);
        let directory = Directory::new();
        let (mut store, effect) = populated(&directory);
        let before = store.state.clone();
        let oversized = format!("{request}x");
        assert_eq!(
            allocate(&mut store, &effect, &oversized, None)
                .unwrap_err()
                .code,
            "store/execution_limit"
        );
        assert!(fsm_store::snapshot::store_states_eq(&before, &store.state));
        assert_eq!(store.journal.last_seq, before.last_seq);
        let first = allocate(&mut store, &effect, &request, None).unwrap();
        assert_eq!(store.state.execution.run_high_water(), 1);
        drop(store);
        let mut store = Store::open(&directory.0).unwrap();
        let duplicate = allocate(&mut store, &effect, &request, None).unwrap();
        assert_eq!(duplicate.get("duplicate"), Some(&Value::Bool(true)));
        assert_eq!(duplicate.get("execution"), first.get("execution"));
        assert_eq!(store.state.execution.run_high_water(), 1);
        assert!(!store.state.dedup.contains_key(&oversized));
    }
}

#[test]
fn production_claim_metadata_accepts_exact_canonical_limit_before_reopen() {
    let baseline = Directory::new();
    let (mut template_store, effect) = populated(&baseline);
    allocate(&mut template_store, &effect, "template", None).unwrap();
    let Value::Obj(mut template) = template_store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .to_value()
    else {
        panic!("claim metadata must be an object");
    };
    // Instance IDs appear twice: directly and within the emitted effect ID.
    let suffix = effect.strip_prefix("instance").unwrap();
    template.insert("instance_id".into(), Value::Str(String::new()));
    template.insert("effect_id".into(), Value::Str(suffix.into()));
    let Value::Obj(mut retry) = policy().to_value() else {
        panic!("retry policy must be an object");
    };
    let base_length = canon_bytes(&Value::Obj(template.clone())).len();
    let backoff = if (4096 - base_length).is_multiple_of(2) {
        40
    } else {
        400
    };
    retry.insert("max_backoff_ms".into(), Value::Num(backoff.to_string()));
    template.insert("retry".into(), Value::Obj(retry.clone()));
    let overhead = canon_bytes(&Value::Obj(template)).len();
    assert!((4096 - overhead).is_multiple_of(2));
    let instance_id = "i".repeat((4096 - overhead) / 2);
    let exact_policy = RetryPolicy::from_value(&Value::Obj(retry.clone())).unwrap();
    retry.insert(
        "max_backoff_ms".into(),
        Value::Num((backoff * 10).to_string()),
    );
    let oversized_policy = RetryPolicy::from_value(&Value::Obj(retry)).unwrap();
    drop(template_store);

    let directory = Directory::new();
    let (mut store, effect) = populated_named(&directory, &instance_id);
    let before = store.state.clone();
    let native = domain();
    let fingerprint = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut clock = FixedClock::new(100, 1);
    assert_eq!(
        store
            .claim_execution_on(
                &mut clock,
                ExecutionClaimRequest {
                    instance_id: &instance_id,
                    effect_id: &effect,
                    handler_fingerprint: fingerprint,
                    retry: &oversized_policy,
                    domain: &native,
                    request_id: "too-large",
                    expected_seq: None,
                }
            )
            .unwrap_err()
            .code,
        "store/execution_limit"
    );
    assert!(fsm_store::snapshot::store_states_eq(&before, &store.state));
    assert_eq!(store.journal.last_seq, before.last_seq);
    assert!(!store.state.dedup.contains_key("too-large"));
    store
        .claim_execution_on(
            &mut clock,
            ExecutionClaimRequest {
                instance_id: &instance_id,
                effect_id: &effect,
                handler_fingerprint: fingerprint,
                retry: &exact_policy,
                domain: &native,
                request_id: "exact",
                expected_seq: None,
            },
        )
        .unwrap();
    let claim = store
        .state
        .execution
        .claim_for(&instance_id, &effect)
        .unwrap()
        .clone();
    assert_eq!(canon_bytes(&claim.to_value()).len(), 4096);
    let Value::Obj(mut oversized_metadata) = claim.to_value() else {
        panic!("claim metadata must be an object");
    };
    oversized_metadata.insert("retry".into(), oversized_policy.to_value());
    assert_eq!(
        canon_bytes(&Value::Obj(oversized_metadata.clone())).len(),
        4097
    );
    assert_eq!(claim.run_id(), 1);
    let record = store.records.last().unwrap().clone();
    let cache = fsm_store::snapshot::write_snapshot(&directory.0, &store.state).unwrap();
    let Value::Obj(mut snapshot) = fsm_store::snapshot::state_to_snapshot(&store.state) else {
        panic!("snapshot must be an object");
    };
    let Value::Obj(mut execution) = store.state.execution.to_value() else {
        panic!("execution block must be an object");
    };
    let Some(Value::Arr(claims)) = execution.get_mut("claims") else {
        panic!("execution claims must be an array");
    };
    let Value::Obj(entry) = &mut claims[0] else {
        panic!("execution entry must be an object");
    };
    entry.insert("claim".into(), Value::Obj(oversized_metadata));
    snapshot.insert("execution".into(), Value::Obj(execution));
    snapshot.insert("snapshot_hash".into(), Value::Str(String::new()));
    let hash = fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
        "fsm:snapshot:6",
        &Value::Obj(snapshot.clone()),
    ));
    snapshot.insert("snapshot_hash".into(), Value::Str(format!("sha256:{hash}")));
    let hostile_snapshot = Value::Obj(snapshot);
    assert_eq!(
        fsm_store::snapshot::snapshot_to_state(&hostile_snapshot)
            .unwrap_err()
            .message,
        "invalid execution field: bytes"
    );
    drop(store);
    let store = Store::open(&directory.0).unwrap();
    assert_eq!(
        store.state.execution.claim_for(&instance_id, &effect),
        Some(&claim)
    );
    assert_eq!(store.state.execution.run_high_water(), 1);
    drop(store);
    let healthy_cache = std::fs::read(&cache).unwrap();
    let hostile_bytes = canon_bytes(&hostile_snapshot);
    std::fs::write(&cache, &hostile_bytes).unwrap();
    let read_only = Store::open_read_only(&directory.0).unwrap();
    assert_eq!(
        read_only.state.execution.claim_for(&instance_id, &effect),
        Some(&claim)
    );
    assert_eq!(std::fs::read(&cache).unwrap(), hostile_bytes);
    drop(read_only);
    std::fs::write(&cache, healthy_cache).unwrap();

    // Keep the chain hash valid so refusal cannot be attributed to hash damage.
    let segment = directory.0.join("journal/seg-00000000000000000000.jsonl");
    let healthy_journal = std::fs::read(&segment).unwrap();
    let original_line = record.to_line();
    assert!(healthy_journal.ends_with(&original_line));
    let Value::Obj(mut body) = record.body.clone() else {
        panic!("claim body must be an object");
    };
    body.insert("retry".into(), oversized_policy.to_value());
    let mut hostile_journal =
        healthy_journal[..healthy_journal.len() - original_line.len()].to_vec();
    hostile_journal.extend(
        fsm_core::record::seal(
            record.seq,
            record.ts,
            record.kind,
            Value::Obj(body),
            &record.prev,
        )
        .to_line(),
    );
    std::fs::write(&segment, &hostile_journal).unwrap();
    assert!(Store::open_read_only(&directory.0).is_err());
    assert_eq!(std::fs::read(&segment).unwrap(), hostile_journal);
    assert!(Store::open(&directory.0).is_err());
    assert_eq!(std::fs::read(&segment).unwrap(), hostile_journal);
    std::fs::write(&segment, healthy_journal).unwrap();
    let mut restored = Store::open(&directory.0).unwrap();
    assert_eq!(
        restored.state.execution.claim_for(&instance_id, &effect),
        Some(&claim)
    );
    restored
        .cancel_instance(&instance_id, "cancel-before-seal")
        .unwrap();
    let archive = directory.0.join("metadata-archive");
    std::fs::create_dir_all(&archive).unwrap();
    restored.seal_and_archive(&archive, None).unwrap();
    fsm_store::archive::verify(&archive).unwrap();
    let opened = fsm_store::base::open_from_base(&directory.0, &restored.records).unwrap();
    assert_eq!(
        opened.state.execution.claim_for(&instance_id, &effect),
        Some(&claim)
    );
    let roots = fsm_store::base::base_roots(&opened.state, &opened.index);
    let Value::Obj(mut base) = fsm_store::base::read_value(&directory.0).unwrap() else {
        panic!("base must be an object");
    };
    let Value::Obj(mut execution) = opened.state.execution.to_value() else {
        panic!("execution block must be an object");
    };
    let Some(Value::Arr(claims)) = execution.get_mut("claims") else {
        panic!("execution claims must be an array");
    };
    let Value::Obj(entry) = &mut claims[0] else {
        panic!("execution entry must be an object");
    };
    let Value::Obj(mut metadata) = claim.to_value() else {
        panic!("claim metadata must be an object");
    };
    metadata.insert("retry".into(), oversized_policy.to_value());
    entry.insert("claim".into(), Value::Obj(metadata));
    base.insert("execution".into(), Value::Obj(execution));
    let hostile_base = Value::Obj(base);
    assert_eq!(
        fsm_store::base::decode(&hostile_base, &roots)
            .unwrap_err()
            .message,
        "base state file: invalid execution field: bytes"
    );
    let path = fsm_store::base::base_path(&directory.0);
    drop(restored);
    let original_base = std::fs::read(&path).unwrap();
    let hostile_base_bytes = canon_bytes(&hostile_base);
    std::fs::write(&path, &hostile_base_bytes).unwrap();
    assert!(Store::open_read_only(&directory.0).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), hostile_base_bytes);
    assert!(Store::open(&directory.0).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), hostile_base_bytes);
    std::fs::write(&path, original_base).unwrap();
    let restored = Store::open(&directory.0).unwrap();
    assert!(restored.sealed_open);
    assert_eq!(
        restored.state.execution.claim_for(&instance_id, &effect),
        Some(&claim)
    );
}
