//! Opt-in real privileged allocator cases, never portable skip evidence.

use super::*;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::record::execution::RetryPolicy;
use fsm_store::clock::FixedClock;
use fsm_store::store::{ExecutionClaimRequest, Store};
use std::collections::BTreeMap;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::time::{Duration, Instant};

#[path = "admission_native_tests.rs"]
mod admission_cases;

#[path = "termination_native_tests.rs"]
mod termination_cases;

#[path = "enrollment_native_tests.rs"]
mod enrollment_cases;

#[path = "runner_native_tests.rs"]
mod runner_cases;

#[path = "broker_native_tests.rs"]
mod broker_cases;

#[path = "supervisor_native_probe.rs"]
mod supervisor_probe;

#[test]
#[ignore = "requires provisioned root broker and writable cgroups"]
fn provisioned_broker_access() {
    broker_cases::run();
}

#[test]
#[ignore = "requires provisioned root broker and writable cgroups"]
fn provisioned_broker_disconnect() {
    broker_cases::disconnect();
}

struct Fixture {
    directory: PathBuf,
    store: PathBuf,
    groups: Vec<(PathBuf, Value)>,
    created_base: bool,
}

impl Fixture {
    fn new() -> Self {
        Self::new_for_table(approved_table())
    }

    fn new_for_table(table: Value) -> Self {
        Self::new_for_table_location(table, false)
    }

    fn new_for_operator(table: Value) -> Self {
        Self::new_for_table_location(table, true)
    }

    fn new_for_table_location(table: Value, operator_store: bool) -> Self {
        assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
        protected_directory(Path::new(GROUPS)).unwrap();
        let seed = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let namespace =
            fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(seed.as_bytes()))[..32].to_owned();
        let directory = super::super::authority_path(&namespace, "1").unwrap();
        let created_base = !Path::new(super::super::BASE).exists();
        if created_base {
            fs::DirBuilder::new()
                .mode(0o755)
                .create(super::super::BASE)
                .unwrap();
        }
        assert!(!directory.parent().unwrap().exists());
        let store = if operator_store {
            // This test-only store must be reachable after dropping all Root
            // credentials; shared temporary ancestors may be private to Root.
            fs::DirBuilder::new()
                .mode(0o755)
                .create(directory.parent().unwrap())
                .unwrap();
            fs::set_permissions(
                directory.parent().unwrap(),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            directory.parent().unwrap().join("operator-store")
        } else {
            std::env::temp_dir().join(format!("fsm-native-authority-store-{namespace}"))
        };
        let fixture = Self {
            directory,
            store,
            groups: Vec::new(),
            created_base,
        };
        drop(Store::open(&fixture.store).unwrap());
        super::super::register(&fixture.directory, &fixture.store).unwrap();
        let public = fixture.directory.join("store-identity.json");
        let saved = fixture.directory.join("fixture-store-identity.saved");
        let encoded = fs::read(&public).unwrap();
        assert!(require_unused(&fixture.directory).is_ok());
        fs::rename(&public, &saved).unwrap();
        assert!(require_unused(&fixture.directory).is_err());
        fs::rename(&saved, &public).unwrap();
        fs::set_permissions(&public, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(require_unused(&fixture.directory).is_err());
        fs::set_permissions(&public, fs::Permissions::from_mode(0o444)).unwrap();
        fs::write(&public, b"{").unwrap();
        assert!(require_unused(&fixture.directory).is_err());
        fs::write(
            &public,
            canon_bytes(&object([
                ("format", Value::Str("fsm.native-store-identity/1".into())),
                (
                    "identity",
                    object([
                        ("device", Value::Num("0".into())),
                        ("inode", Value::Num("0".into())),
                    ]),
                ),
            ])),
        )
        .unwrap();
        assert!(require_unused(&fixture.directory).is_err());
        fs::write(&public, &encoded).unwrap();
        assert!(require_unused(&fixture.directory).is_ok());
        assert_eq!(fs::read(&public).unwrap(), encoded);
        assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 0);
        assert!(!fixture.directory.join("catalogue.json").exists());
        assert!(!fixture.directory.join("allocation-1.json").exists());
        let before = fixture.counter();
        assert!(prepare(&fixture.directory).is_err());
        assert_eq!(fixture.counter(), before);
        assert!(!fixture.directory.join("allocation-1.json").exists());
        super::super::catalogue::publish(&fixture.directory, &table).unwrap();
        assert!(super::super::catalogue::publish(&fixture.directory, &table).is_err());
        fixture
    }

    fn prepare(&mut self) -> Value {
        let domain = prepare(&self.directory).unwrap();
        let origin = origin(&self.directory).unwrap();
        let path = cgroup(&origin, number(&domain, "allocation").unwrap()).unwrap();
        assert_eq!(
            domain.get("cgroup"),
            Some(&identity(&fs::metadata(&path).unwrap()))
        );
        assert!(
            fs::read_to_string(path.join("cgroup.events"))
                .unwrap()
                .lines()
                .any(|line| line == "populated 0")
        );
        self.groups
            .push((path, domain.get("cgroup").unwrap().clone()));
        domain
    }

    fn counter(&self) -> Value {
        read_value(&self.directory.join("counter.json"), true).unwrap()
    }

    fn cleanup(&mut self) -> Result<(), String> {
        for (path, expected) in &self.groups {
            if !path.exists() {
                continue;
            }
            let metadata = fs::symlink_metadata(path).map_err(io)?;
            if identity(&metadata) != *expected
                || !fs::read_to_string(path.join("cgroup.events"))
                    .map_err(io)?
                    .lines()
                    .any(|line| line == "populated 0")
            {
                return Err("fixture refuses cleanup of changed or populated domain".into());
            }
            fs::remove_dir(path).map_err(io)?;
        }
        let prefix = prefix(&origin(&self.directory)?)?;
        if fs::read_dir(GROUPS).map_err(io)?.any(|entry| {
            entry.is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix))
            })
        }) {
            return Err("fixture retains authority for an unknown surviving domain".into());
        }
        fs::remove_dir_all(&self.store).map_err(io)?;
        fs::remove_dir_all(self.directory.parent().ok_or("namespace missing")?).map_err(io)?;
        if self.created_base {
            let _ = fs::remove_dir(super::super::BASE);
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.directory.exists() {
            let _ = self.cleanup();
        }
    }
}

#[test]
#[ignore = "requires writable provisioned root cgroups"]
fn empty_domain_preparation() {
    let mut fixture = Fixture::new();
    let domain = fixture.prepare();
    assert_eq!(number(&domain, "allocation").unwrap(), 1);
    let second = fixture.prepare();
    assert_eq!(number(&second, "allocation").unwrap(), 2);
    assert_ne!(domain.get("cgroup"), second.get("cgroup"));
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 2);
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(store.state.execution.run_high_water(), 0);
    assert_eq!(store.state.execution.unresolved().count(), 0);
    drop(store);
    // Exercise residual cleanup on actual kernel resources, independently of
    // systemd's sometimes immediate directory removal; this issues no receipt.
    let group = fixture.groups[0].0.clone();
    let unit = group.file_name().unwrap().to_str().unwrap();
    let deadline = || Instant::now() + Duration::from_secs(2);
    let remove = |material: &Value, manager_unit: &str| {
        super::super::closure::remove_empty(
            &fixture.directory,
            1,
            material,
            manager_unit,
            &group,
            deadline(),
        )
    };
    assert!(remove(&domain, unit).is_err()); // Entry has not been revoked.
    assert!(group.exists());
    let (revoked, lock) = super::super::closing::revoke(&fixture.directory, 1).unwrap();
    assert_eq!(revoked, domain);
    let mut changed = domain.as_obj().unwrap().clone();
    changed.insert("cgroup".into(), second.get("cgroup").unwrap().clone());
    assert!(remove(&Value::Obj(changed), unit).is_err());
    assert!(remove(&domain, "system.slice").is_err()); // Manager still owns it.
    assert!(group.exists());
    let child = group.join("fixture-owned-empty-child");
    fs::create_dir(&child).unwrap();
    let child_identity = identity(&fs::symlink_metadata(&child).unwrap());
    assert!(remove(&domain, unit).is_err()); // Never recursively remove children.
    assert!(group.exists() && child.exists());
    assert_eq!(
        identity(&fs::symlink_metadata(&child).unwrap()),
        child_identity
    );
    fs::remove_dir(&child).unwrap(); // Remove only the test-created obstacle.
    remove(&domain, unit).unwrap();
    assert!(!group.exists());
    assert!(fixture.groups[1].0.exists());
    assert!(!fixture.directory.join("closed-1.json").exists());
    drop(lock);
    fixture.cleanup().unwrap();
}

#[test]
#[ignore = "requires writable provisioned root cgroups"]
fn unknown_domain_refusal() {
    let mut fixture = Fixture::new();
    let origin = origin(&fixture.directory).unwrap();
    let unknown = cgroup(&origin, 99).unwrap();
    fs::create_dir(&unknown).unwrap();
    fixture
        .groups
        .push((unknown.clone(), identity(&fs::metadata(&unknown).unwrap())));
    let before = fixture.counter();
    assert!(
        prepare(&fixture.directory)
            .unwrap_err()
            .contains("unknown native domain")
    );
    assert_eq!(fixture.counter(), before);
    assert!(unknown.exists());
    fixture.cleanup().unwrap();
}

#[test]
#[ignore = "requires writable provisioned root cgroups"]
fn counter_rollback_refusal() {
    let mut fixture = Fixture::new();
    let zero = fixture.counter();
    fixture.prepare();
    let current = fixture.counter();
    fs::write(fixture.directory.join("counter.json"), canon_bytes(&zero)).unwrap();
    assert!(
        prepare(&fixture.directory)
            .unwrap_err()
            .contains("rollback")
    );
    assert_eq!(fixture.counter(), zero);
    fs::write(
        fixture.directory.join("counter.json"),
        canon_bytes(&current),
    )
    .unwrap();
    fixture.cleanup().unwrap();
}

#[test]
#[ignore = "requires writable provisioned root cgroups"]
fn incomplete_intent_refusal() {
    let mut fixture = Fixture::new();
    let origin = origin(&fixture.directory).unwrap();
    // A persisted intermediate state, not a claim of physical power loss.
    publish_once(
        &fixture.directory.join("allocation-1.json"),
        &intent(&origin, 1),
    )
    .unwrap();
    advance(&fixture.directory, fixture.counter(), 1).unwrap();
    let before = fixture.counter();
    assert!(prepare(&fixture.directory).is_err());
    assert_eq!(fixture.counter(), before);
    assert!(!cgroup(&origin, 1).unwrap().exists());
    fixture.cleanup().unwrap();
}

#[test]
#[ignore = "requires writable provisioned root cgroups"]
fn genuine_claim_binding() {
    admission_cases::removed_pending();
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let listing = || {
        let mut names = fs::read_dir(&fixture.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let before = listing();
    let lock = super::super::authority_lock(&fixture.directory).unwrap();
    let observed = super::super::observation::read(&fixture.directory, 1).unwrap();
    assert_eq!(observed.get("domain"), Some(&domain.to_value()));
    assert_eq!(observed.get("closing"), Some(&Value::Bool(false)));
    assert_eq!(observed.get("populated"), Some(&Value::Bool(false)));
    assert_eq!(listing(), before);
    drop(lock);
    let (binding, effect) = claim_binding(&fixture, &domain);
    admission_cases::bound_claim(&fixture, &binding, &effect);
    super::super::bind(&fixture.directory, &binding).unwrap();
    let path = fixture.directory.join("binding-1.json");
    assert_eq!(read_value(&path, true).unwrap(), binding);
    assert!(super::super::bind(&fixture.directory, &binding).is_err());
    exercise_binding(&mut fixture, &domain, &binding, &path, &effect);
    let claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let receipt = fixture
        .directory
        .join(format!("closure-1-{}.json", claim.run_id()));
    for name in [
        "launch-1.json",
        "launch-1.json.pending",
        "handoff-1.json.pending",
        "manager-stopped-1.json.pending",
        "manager-retired-1.json.pending",
    ] {
        let fault = fixture.directory.join(name);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&fault)
            .unwrap();
        assert!(super::super::closure::complete(&fixture.directory, 1).is_err());
        assert!(fsm_store::store::VerifiedClosure::read(&receipt).is_err());
        assert!(fixture.groups[0].0.exists());
        fs::remove_file(fault).unwrap();
    }
    let pending = receipt.with_extension("json.pending");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .unwrap();
    assert!(super::super::closure::complete(&fixture.directory, 1).is_err());
    assert!(fsm_store::store::VerifiedClosure::read(&receipt).is_err());
    assert!(!fixture.groups[0].0.exists());
    // Repair only the test-owned injected receipt obstacle; the failed close
    // remains a refusal, and this independent retry must prove cold closure.
    fs::remove_file(pending).unwrap();
    super::super::closure::complete(&fixture.directory, 1).unwrap();
    super::super::closure::complete(&fixture.directory, 1).unwrap();
    let final_identity = identity(&fs::symlink_metadata(&receipt).unwrap());
    let final_bytes = fs::read(&receipt).unwrap();
    let pending = receipt.with_extension("json.pending");
    fs::copy(&receipt, &pending).unwrap();
    let unrelated_pending = identity(&fs::symlink_metadata(&pending).unwrap());
    assert_ne!(unrelated_pending, final_identity);
    assert!(
        super::super::closure::complete(&fixture.directory, 1)
            .unwrap_err()
            .contains("pending receipt differs")
    );
    assert_eq!(
        identity(&fs::symlink_metadata(&pending).unwrap()),
        unrelated_pending
    );
    assert_eq!(fs::read(&receipt).unwrap(), final_bytes);
    fs::remove_file(&pending).unwrap();
    fs::hard_link(&receipt, &pending).unwrap();
    super::super::closure::complete(&fixture.directory, 1).unwrap();
    assert_eq!(
        fs::symlink_metadata(&pending).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(
        identity(&fs::symlink_metadata(&receipt).unwrap()),
        final_identity
    );
    assert_eq!(fs::read(&receipt).unwrap(), final_bytes);
    assert!(
        fsm_store::store::VerifiedClosure::read(&receipt)
            .unwrap()
            .matches_claim(
                &claim,
                binding.get("journal_claim").unwrap().as_str().unwrap()
            )
    );
    assert!(!fixture.groups[0].0.exists());
    for prefix in ["launch", "handoff", "manager-stopped", "manager-retired"] {
        assert_eq!(
            fs::symlink_metadata(fixture.directory.join(format!("{prefix}-1.json")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    let snapshot = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        snapshot.state.execution.claim_for("instance", &effect),
        Some(&claim)
    );
    assert!(
        snapshot
            .state
            .execution
            .stopped_for("instance", &effect)
            .is_none()
    );
}

fn claim_binding(fixture: &Fixture, domain: &NativeDomain) -> (Value, String) {
    let mut store = Store::open(&fixture.store).unwrap();
    store
        .define_machine(
            parse(
                include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
            false,
            false,
        )
        .unwrap();
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
    let effect = store.state.instances["instance"].pending[0].clone();
    let approved = super::super::catalogue::read(&fixture.directory).unwrap();
    let handler = &approved.handlers["notify"];
    let before = store.records.clone();
    let state = store.state.clone();
    let mut wrong = handler.clone();
    wrong.effect = "different_effect".into();
    let mut pipeline = fsm_execute::run::Pipeline;
    assert_eq!(
        pipeline
            .claim_native_handler(
                &mut store,
                &mut FixedClock::new(100, 1),
                &effect,
                &wrong,
                domain,
                "wrong-handler-claim"
            )
            .unwrap_err()
            .code,
        "exec/config"
    );
    assert_eq!(store.records, before);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert!(!store.state.dedup.contains_key("wrong-handler-claim"));
    let mut invalid = handler.clone();
    invalid.retry.attempts = 0;
    assert_eq!(
        pipeline
            .claim_native_handler(
                &mut store,
                &mut FixedClock::new(100, 1),
                &effect,
                &invalid,
                domain,
                "invalid-handler-claim"
            )
            .unwrap_err()
            .code,
        "exec/config"
    );
    assert_eq!(store.records, before);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert!(!store.state.dedup.contains_key("invalid-handler-claim"));
    let claim = pipeline
        .claim_native_handler(
            &mut store,
            &mut FixedClock::new(100, 1),
            &effect,
            handler,
            domain,
            "claim",
        )
        .unwrap();
    assert_eq!(
        claim,
        *store
            .state
            .execution
            .claim_for("instance", &effect)
            .unwrap()
    );
    let records = store.records.clone();
    let hash = store.current_execution_claim_hash(&claim).unwrap();
    let duplicate = pipeline
        .claim_native_handler(
            &mut store,
            &mut FixedClock::new(100, 1),
            &effect,
            handler,
            domain,
            "claim",
        )
        .unwrap();
    assert_eq!(duplicate, claim);
    assert_eq!(store.records, records);
    assert_eq!(store.current_execution_claim_hash(&claim).unwrap(), hash);
    let binding = object([
        ("format", Value::Str("fsm.native-claim-binding/1".into())),
        (
            "claim",
            store
                .state
                .execution
                .claim_for("instance", &effect)
                .unwrap()
                .to_value(),
        ),
        (
            "journal_claim",
            Value::Str(
                store
                    .current_execution_claim_hash(
                        store
                            .state
                            .execution
                            .claim_for("instance", &effect)
                            .unwrap(),
                    )
                    .unwrap(),
            ),
        ),
    ]);
    drop(store);
    (binding, effect)
}

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn enrolled_gate_authorization() {
    enrollment_cases::run();
}

fn exercise_binding(
    fixture: &mut Fixture,
    domain: &NativeDomain,
    binding: &Value,
    path: &Path,
    effect: &str,
) {
    let grant = object([
        ("format", Value::Str("fsm.native-entry/1".into())),
        ("claim", binding.get("claim").unwrap().clone()),
        (
            "journal_claim",
            binding.get("journal_claim").unwrap().clone(),
        ),
        ("argv", Value::Arr(vec![Value::Str("/bin/true".into())])),
    ]);
    let request = object([
        ("grant", grant.clone()),
        ("group_id", Value::Num("1".into())),
    ]);
    let root_group = object([
        ("grant", grant.clone()),
        ("group_id", Value::Num("0".into())),
    ]);
    assert!(super::super::authorize::publish(&fixture.directory, &root_group).is_err());
    let mut wrong_grant = grant.as_obj().unwrap().clone();
    wrong_grant.insert(
        "argv".into(),
        Value::Arr(vec![Value::Str("/bin/false".into())]),
    );
    let wrong_request = object([
        ("grant", Value::Obj(wrong_grant)),
        ("group_id", Value::Num("1".into())),
    ]);
    assert!(
        super::super::authorize::publish(&fixture.directory, &wrong_request)
            .unwrap_err()
            .contains("approved journal-derived handler")
    );
    assert!(!fixture.directory.join("entry-1.json").exists());
    let enrolled_request = object([("grant", grant.clone())]);
    assert!(super::super::authorize::publish_enrolled(&fixture.directory, &request).is_err());
    // A genuine prepared/claimed domain is not an enrolled manager gate.
    assert!(
        super::super::authorize::publish_enrolled(&fixture.directory, &enrolled_request).is_err()
    );
    assert!(!fixture.directory.join("entry-1.json").exists());
    assert!(!fixture.directory.join("entry-1.json.pending").exists());
    assert!(super::super::catalogue::publish(&fixture.directory, &approved_table()).is_err());
    assert!(
        super::super::entry::wait_grant(&fixture.directory, 1, std::time::Duration::ZERO)
            .unwrap_err()
            .contains("deadline expired")
    );
    for phase in ["closing", "closed"] {
        let marker = fixture.directory.join(format!("{phase}-1.json"));
        fs::write(&marker, b"malformed marker").unwrap();
        assert!(
            super::super::authorize::publish(&fixture.directory, &request)
                .unwrap_err()
                .contains("closing or closed")
        );
        assert!(!fixture.directory.join("entry-1.json").exists());
        assert!(
            super::super::entry::wait_grant(&fixture.directory, 1, std::time::Duration::ZERO)
                .unwrap_err()
                .contains("closing or closed")
        );
        fs::remove_file(marker).unwrap();
    }
    super::super::authorize::publish(&fixture.directory, &request).unwrap();
    let entry_path = fixture.directory.join("entry-1.json");
    let metadata = fs::metadata(&entry_path).unwrap();
    assert_eq!(
        (metadata.uid(), metadata.gid(), metadata.mode() & 0o777),
        (0, 1, 0o440)
    );
    assert_eq!(read_value(&entry_path, true).unwrap(), grant);
    assert_eq!(
        super::super::entry::wait_grant(&fixture.directory, 1, std::time::Duration::ZERO).unwrap(),
        grant
    );
    assert!(super::super::authorize::publish(&fixture.directory, &request).is_err());
    unapproved_claim_is_not_bound(fixture);
    let mut store = Store::open(&fixture.store).unwrap();
    store.cancel_instance("instance", "cancel").unwrap();
    drop(store);
    assert!(
        super::super::authorize::publish(&fixture.directory, &request)
            .unwrap_err()
            .contains("runnable ownership")
    );
    assert_eq!(read_value(&entry_path, true).unwrap(), grant);
    assert!(
        super::super::bind(&fixture.directory, binding)
            .unwrap_err()
            .contains("runnable ownership")
    );
    assert_eq!(&read_value(path, true).unwrap(), binding);
    let pending = fixture.directory.join("entry-1.json.pending");
    fs::remove_file(&pending).unwrap();
    std::os::unix::fs::symlink(path, &pending).unwrap();
    assert!(
        super::super::closing::begin(&fixture.directory, 1)
            .unwrap_err()
            .contains("unexpected grant ownership or type")
    );
    assert!(!entry_path.exists());
    assert!(
        fs::symlink_metadata(&pending)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(&read_value(path, true).unwrap(), binding);
    assert!(fixture.directory.join("closing-1.json").exists());
    assert!(super::super::authorize::publish(&fixture.directory, &request).is_err());
    fs::remove_file(&pending).unwrap();
    super::super::closing::begin(&fixture.directory, 1).unwrap();
    assert!(!entry_path.exists());
    assert!(!fixture.directory.join("entry-1.json.pending").exists());
    assert!(!fixture.directory.join("closed-1.json").exists());
    let closing_path = fixture.directory.join("closing-1.json");
    let closing = read_value(&closing_path, true).unwrap();
    assert_eq!(closing.get("domain"), Some(&domain.to_value()));
    super::super::closing::begin(&fixture.directory, 1).unwrap();
    assert_eq!(read_value(&closing_path, true).unwrap(), closing);
    assert!(super::super::authorize::publish(&fixture.directory, &request).is_err());
    termination_cases::members(fixture);
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", effect)
            .is_some()
    );
    drop(store);
    fixture.cleanup().unwrap();
}

fn approved_table() -> Value {
    parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#,
        &JsonLimits::DEFAULT).unwrap()
}

fn unapproved_claim_is_not_bound(fixture: &mut Fixture) {
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let mut store = Store::open(&fixture.store).unwrap();
    store
        .create_instance("case_review", "unapproved", "unapproved-create", None)
        .unwrap();
    store
        .send_event(
            "unapproved",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "unapproved-send",
            None,
        )
        .unwrap();
    let effect = store.state.instances["unapproved"].pending[0].clone();
    let retry = RetryPolicy::new(1, 10, 10, Vec::new()).unwrap();
    store
        .claim_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionClaimRequest {
                instance_id: "unapproved",
                effect_id: &effect,
                handler_fingerprint: &format!("sha256:{}", "a".repeat(64)),
                retry: &retry,
                domain: &domain,
                request_id: "unapproved-claim",
                expected_seq: None,
            },
        )
        .unwrap();
    let binding = object([
        ("format", Value::Str("fsm.native-claim-binding/1".into())),
        (
            "claim",
            store
                .state
                .execution
                .claim_for("unapproved", &effect)
                .unwrap()
                .to_value(),
        ),
        (
            "journal_claim",
            Value::Str(format!("sha256:{}", store.records.last().unwrap().hash)),
        ),
    ]);
    drop(store);
    assert!(
        super::super::bind(&fixture.directory, &binding)
            .unwrap_err()
            .contains("approved handler identity or retry differs")
    );
    assert!(!fixture.directory.join("binding-2.json").exists());
    assert!(!fixture.directory.join("entry-2.json").exists());
}
