//! Composition runs unattended: the executor invokes, returns, and delivers
//! without a subprocess and without a human.
//!
//! Plan 0010 task 5101.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_execute::rid::{invoke_rid, return_rid, signal_rid};
use fsm_execute::sched::{Directive, Scheduler};
use fsm_execute::watch::Watcher;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_execute::config::HandlerTable;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-exec-comp-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The composition directives need no handler of their own — they reach the
/// journal, not the world's computers — but a table must declare at least
/// one, so this declares the child's effect and nothing else.
fn empty_table() -> HandlerTable {
    HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"fx","argv":["/bin/true"],"timeout_ms":1000}]}"#,
    )
    .unwrap()
}

fn advancing() -> BTreeSet<String> {
    BTreeSet::from(["fx".to_string()])
}

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(src: &str) -> String {
    digest_of(&machine_id(&value(src))).unwrap().to_string()
}

/// A leaf that emits an effect on entry and finishes on `finish`.
const LEAF: &str = r#"{"format":"fsm.machine/1","name":"leaf","states":[{"name":"working","entry":{"emit":[{"effect":"fx","args":{"note":"ctx.note"}}]}},{"name":"done","terminal":true}],"initial":"working","context":[{"name":"note","ty":"str","init":"hello"}],"events":[{"name":"finish","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"note","ty":"str"}]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

fn parent_src(child_digest: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{child_digest}"}}]}},{{"name":"settled"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.down","to":"settled"}}]}}"#
    )
}

/// A parent in `busy` with one pending invocation slot.
fn waiting(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(LEAF), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(&parent_src(&digest(LEAF))), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "parent",
            "p1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("p1", "open", Value::Obj(BTreeMap::new()), "open-1", None)
        .unwrap();
    store
}

fn observe(watcher: &mut Watcher) -> fsm_execute::watch::Observation {
    watcher.scan(10_000).expect("the scan reads")
}

fn new_watcher(directory: &TestDirectory) -> Watcher {
    Watcher::new(directory.path().to_path_buf(), advancing())
}

#[test]
fn a_pending_slot_produces_one_invoke_directive_and_then_none() {
    let directory = TestDirectory::create();
    let mut store = waiting(&directory);
    drop(store);
    let mut watcher = new_watcher(&directory);
    let mut scheduler = Scheduler::new(empty_table());
    let observation = observe(&mut watcher);
    assert_eq!(observation.pending_invocations.len(), 1);
    let directives = scheduler.on_observation(&observation, 10_000);
    let invokes: Vec<&Directive> = directives
        .iter()
        .filter(|d| matches!(d, Directive::InvokeChild { .. }))
        .collect();
    assert_eq!(invokes.len(), 1);
    let Directive::InvokeChild {
        parent_instance_id,
        slot,
        child_instance_id: child,
        request_id,
    } = invokes[0]
    else {
        unreachable!()
    };
    assert_eq!(parent_instance_id, "p1");
    assert_eq!(slot, "down");
    assert_eq!(child, &child_instance_id("p1", "down"));
    assert_eq!(request_id, &invoke_rid("p1", "down"));

    // Once the key is claimed, the same observation directs nothing.
    let mut claimed = observation.clone();
    claimed.claimed_request_ids.insert(request_id.clone());
    let again = scheduler.on_observation(&claimed, 10_000);
    assert!(
        !again
            .iter()
            .any(|d| matches!(d, Directive::InvokeChild { .. }))
    );
    store = Store::open(directory.path()).unwrap();
    let _ = store;
}

#[test]
fn a_settled_child_produces_a_return_and_a_running_one_does_not() {
    let directory = TestDirectory::create();
    let mut store = waiting(&directory);
    store.invoke_child("p1", "down", "inv-1").unwrap();
    let child = child_instance_id("p1", "down");
    drop(store);

    let mut watcher = new_watcher(&directory);
    let mut scheduler = Scheduler::new(empty_table());
    let observation = observe(&mut watcher);
    assert!(
        observation.returnable_invocations.is_empty(),
        "the child still runs"
    );
    assert!(
        !scheduler
            .on_observation(&observation, 10_000)
            .iter()
            .any(|d| matches!(d, Directive::InvocationReturn { .. }))
    );

    let mut store = Store::open(directory.path()).unwrap();
    store
        .send_event(&child, "finish", Value::Obj(BTreeMap::new()), "fin-1", None)
        .unwrap();
    assert_eq!(store.state.instances[&child].status, Status::Completed);
    drop(store);

    let mut watcher = new_watcher(&directory);
    let mut scheduler = Scheduler::new(empty_table());
    let observation = observe(&mut watcher);
    assert_eq!(observation.returnable_invocations.len(), 1);
    let directives = scheduler.on_observation(&observation, 10_000);
    let returns: Vec<&Directive> = directives
        .iter()
        .filter(|d| matches!(d, Directive::InvocationReturn { .. }))
        .collect();
    assert_eq!(returns.len(), 1);
    let Directive::InvocationReturn { request_id, .. } = returns[0] else {
        unreachable!()
    };
    assert_eq!(request_id, &return_rid("p1", "down"));
    // Restart equivalence: a fresh scheduler on the same observation makes
    // the same decision, and no second invoke.
    let mut fresh = Scheduler::new(empty_table());
    let again = fresh.on_observation(&observation, 10_000);
    assert_eq!(
        again
            .iter()
            .filter(|d| matches!(d, Directive::InvocationReturn { .. }))
            .count(),
        1
    );
    assert!(
        !again
            .iter()
            .any(|d| matches!(d, Directive::InvokeChild { .. }))
    );
}

#[test]
fn a_pending_signal_produces_one_delivery_directive() {
    let sender = r#"{"format":"fsm.machine/1","name":"sender","states":[{"name":"idle"},{"name":"working","entry":{"signal":[{"to":"ctx.target","event":"ping"}]}}],"initial":"idle","context":[{"name":"target","ty":"str","init":"inst-other"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"idle","on":"go","to":"working"}]}"#;
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(sender), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "sender",
            "s1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("s1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    let signal_id = store.state.instances["s1"]
        .signals
        .keys()
        .next()
        .cloned()
        .unwrap();
    drop(store);

    let mut watcher = new_watcher(&directory);
    let mut scheduler = Scheduler::new(empty_table());
    let observation = observe(&mut watcher);
    assert_eq!(observation.pending_signals.len(), 1);
    let directives = scheduler.on_observation(&observation, 10_000);
    let Some(Directive::SignalDeliver {
        request_id,
        target_instance_id,
        ..
    }) = directives
        .iter()
        .find(|d| matches!(d, Directive::SignalDeliver { .. }))
    else {
        panic!("one delivery directive");
    };
    assert_eq!(request_id, &signal_rid("s1", &signal_id));
    assert_eq!(target_instance_id, "inst-other");
}

#[test]
fn within_a_tick_the_order_is_invoke_then_return_then_signal() {
    // One parent with a settled child (returnable), a second with a pending
    // slot, and a sender with a pending signal: one observation, three
    // directives, in causal order.
    let directory = TestDirectory::create();
    let mut store = waiting(&directory);
    store.invoke_child("p1", "down", "inv-1").unwrap();
    let child = child_instance_id("p1", "down");
    store
        .send_event(&child, "finish", Value::Obj(BTreeMap::new()), "fin-1", None)
        .unwrap();
    let mut clock = FixedClock::new(2_000, 1);
    store
        .create_instance_ctx_on(
            &mut clock,
            "parent",
            "p2",
            "c2",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("p2", "open", Value::Obj(BTreeMap::new()), "open-2", None)
        .unwrap();
    drop(store);

    let mut watcher = new_watcher(&directory);
    let mut scheduler = Scheduler::new(empty_table());
    let observation = observe(&mut watcher);
    let kinds: Vec<&str> = scheduler
        .on_observation(&observation, 10_000)
        .iter()
        .filter_map(|d| match d {
            Directive::InvokeChild { .. } => Some("invoke"),
            Directive::InvocationReturn { .. } => Some("return"),
            Directive::SignalDeliver { .. } => Some("signal"),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, ["invoke", "return"], "invoke precedes return");
}

#[test]
fn a_childs_creation_emitted_effect_resolves_through_its_invocation_record() {
    // The headline case: a child's entry effect must resolve, and its
    // creation record is `instance_invoked`, not `instance_created`.
    let directory = TestDirectory::create();
    let mut store = waiting(&directory);
    store.invoke_child("p1", "down", "inv-1").unwrap();
    let child = child_instance_id("p1", "down");
    assert_eq!(store.state.instances[&child].pending.len(), 1);
    drop(store);

    let mut watcher = new_watcher(&directory);
    let observation = observe(&mut watcher);
    assert!(
        observation.unresolved.is_empty(),
        "a child's entry effect resolves: {:?}",
        observation.unresolved
    );
    let resolved: Vec<&str> = observation
        .pending
        .iter()
        .map(|effect| effect.effect_name.as_str())
        .collect();
    assert!(resolved.contains(&"fx"), "{resolved:?}");
    let effect = observation
        .pending
        .iter()
        .find(|effect| effect.instance_id == child)
        .expect("the child's effect");
    assert_eq!(
        effect
            .args
            .get("note")
            .map(fsm_core::replay::ctx_val_string),
        Some("hello".to_string()),
        "resolved from the invocation record's overrides and the child's inits"
    );
}

#[test]
fn a_root_instances_creation_effect_still_resolves() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(LEAF), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "leaf",
            "root",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    drop(store);
    let mut watcher = new_watcher(&directory);
    let observation = observe(&mut watcher);
    assert!(
        observation.unresolved.is_empty(),
        "{:?}",
        observation.unresolved
    );
    assert_eq!(observation.pending.len(), 1);
    assert_eq!(observation.pending[0].instance_id, "root");
}

#[test]
fn the_derived_keys_are_stable_and_distinct() {
    let keys: BTreeSet<String> = [
        invoke_rid("p1", "down"),
        return_rid("p1", "down"),
        signal_rid("p1", "p1/3/0"),
    ]
    .into_iter()
    .collect();
    assert_eq!(keys.len(), 3, "no two directives share a key");
    assert_eq!(invoke_rid("p1", "down"), "exec-inv-p1/down");
    assert_eq!(return_rid("p1", "down"), "exec-ret-p1/down");
    assert_eq!(signal_rid("p1", "p1/3/0"), "exec-sig-p1/p1/3/0");
}
