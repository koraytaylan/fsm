//! Opt-in real privileged allocator cases, never portable skip evidence.

use super::*;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::record::execution::RetryPolicy;
use fsm_store::clock::FixedClock;
use fsm_store::store::{ExecutionClaimRequest, Store};
use std::collections::BTreeMap;

#[path = "termination_native_tests.rs"]
mod termination_cases;

struct Fixture {
    directory: PathBuf,
    store: PathBuf,
    groups: Vec<(PathBuf, Value)>,
    created_base: bool,
}

impl Fixture {
    fn new() -> Self {
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
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o755)
                .create(super::super::BASE)
                .unwrap();
        }
        assert!(!directory.parent().unwrap().exists());
        let store = std::env::temp_dir().join(format!("fsm-native-authority-store-{namespace}"));
        let fixture = Self {
            directory,
            store,
            groups: Vec::new(),
            created_base,
        };
        drop(Store::open(&fixture.store).unwrap());
        super::super::register(&fixture.directory, &fixture.store).unwrap();
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
        fs::remove_dir_all(self.directory.parent().ok_or("namespace missing")?).map_err(io)?;
        fs::remove_dir_all(&self.store).map_err(io)?;
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
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
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
    let retry = RetryPolicy::new(1, 10, 10, Vec::new()).unwrap();
    store
        .claim_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionClaimRequest {
                instance_id: "instance",
                effect_id: &effect,
                handler_fingerprint: &format!("sha256:{}", "a".repeat(64)),
                retry: &retry,
                domain: &domain,
                request_id: "claim",
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
                .claim_for("instance", &effect)
                .unwrap()
                .to_value(),
        ),
        (
            "journal_claim",
            Value::Str(format!("sha256:{}", store.records.last().unwrap().hash)),
        ),
    ]);
    drop(store);
    super::super::bind(&fixture.directory, &binding).unwrap();
    let path = fixture.directory.join("binding-1.json");
    assert_eq!(read_value(&path, true).unwrap(), binding);
    assert!(super::super::bind(&fixture.directory, &binding).is_err());
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
        super::super::bind(&fixture.directory, &binding)
            .unwrap_err()
            .contains("runnable ownership")
    );
    assert_eq!(read_value(&path, true).unwrap(), binding);
    let pending = fixture.directory.join("entry-1.json.pending");
    fs::remove_file(&pending).unwrap();
    std::os::unix::fs::symlink(&path, &pending).unwrap();
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
    assert_eq!(read_value(&path, true).unwrap(), binding);
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
    termination_cases::members(&fixture);
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    fixture.cleanup().unwrap();
}
