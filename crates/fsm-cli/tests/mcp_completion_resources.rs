//! Completing the ids behind the three resource templates.
//!
//! Plan 0013 task 6302.

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::complete::complete;
use fsm_cli::mcp::resources;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

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

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-cmpl-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn machine(name: &str) -> Value {
    value(&format!(
        r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"open"}},{{"name":"held"}}],"initial":"open","context":[],"events":[{{"name":"push","fields":[]}}],"transitions":[{{"from":"open","on":"push","to":"held"}},{{"from":"held","on":"push","to":"open"}}]}}"#
    ))
}

fn ask(uri: &str, argument: &str, prefix: &str, store: &Store) -> Value {
    let request = value(&format!(
        r#"{{"ref":{{"type":"ref/resource","uri":"{uri}"}},"argument":{{"name":"{argument}","value":"{prefix}"}}}}"#
    ));
    complete(Some(&request), Some(store)).expect("a well-formed request")
}

fn values(result: &Value) -> Vec<String> {
    result
        .get("completion")
        .and_then(|c| c.get("values"))
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn total(result: &Value) -> usize {
    result
        .get("completion")
        .and_then(|c| c.get("total"))
        .and_then(Value::as_num)
        .unwrap()
        .parse()
        .unwrap()
}

fn has_more(result: &Value) -> bool {
    result
        .get("completion")
        .and_then(|c| c.get("hasMore"))
        .and_then(Value::as_bool)
        .unwrap()
}

/// Five machines, defined oldest to newest.
fn five_machines(dir: &Scratch) -> (Store, Vec<String>) {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    let mut ids = Vec::new();
    for n in 0..5 {
        let outcome = store
            .define_machine_on(&mut clock, machine(&format!("cmpl_{n}")), false, false)
            .unwrap();
        ids.push(outcome.machine_id);
    }
    ids.reverse(); // newest first, which is the order a listing shows
    (store, ids)
}

#[test]
fn machine_ids_come_back_newest_first() {
    let dir = scratch("machines");
    let (store, newest_first) = five_machines(&dir);
    let result = ask("fsm://machine/{id}", "id", "", &store);
    assert_eq!(values(&result), newest_first);
    assert_eq!(total(&result), 5);
    assert!(!has_more(&result));
}

#[test]
fn a_prefix_narrows_to_exactly_its_matches() {
    let dir = scratch("prefix");
    let (store, newest_first) = five_machines(&dir);
    // Two ids that share their first two characters, whatever the hashes are.
    let prefix = &newest_first[0][..2];
    let expected: Vec<String> = newest_first
        .iter()
        .filter(|id| id.starts_with(prefix))
        .cloned()
        .collect();
    let result = ask("fsm://machine/{id}", "id", prefix, &store);
    assert_eq!(values(&result), expected);
    assert_eq!(total(&result), expected.len());

    let result = ask("fsm://machine/{id}", "id", "zzzz", &store);
    assert!(values(&result).is_empty());
    assert_eq!(total(&result), 0, "no matches is an answer, not an error");
    assert!(!has_more(&result));
}

#[test]
fn both_instance_templates_complete_the_same_ids_newest_first() {
    let dir = scratch("instances");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, machine("cmpl_inst"), false, false)
        .unwrap();
    for n in 0..4 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "cmpl_inst",
                &format!("inst-{n}"),
                &format!("create-{n}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let expected = ["inst-3", "inst-2", "inst-1", "inst-0"];
    for uri in ["fsm://instance/{id}", "fsm://instance/{id}/history"] {
        let result = ask(uri, "id", "", &store);
        assert_eq!(values(&result), expected, "{uri}");
    }
}

#[test]
fn a_child_instance_is_offered_like_any_other() {
    // A child has no `instance_created` record — its creation is an
    // `instance_invoked` — so a supplier that scanned for one would hide
    // exactly the instances composition creates.
    let dir = scratch("child");
    let child = machine("cmpl_child");
    let child_id = fsm_core::hashes::digest_of(&fsm_core::hashes::machine_id(&child))
        .unwrap()
        .to_string();
    let parent = value(&format!(
        r#"{{"format":"fsm.machine/1","name":"cmpl_parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{child_id}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.down","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#
    ));
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, child, false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, parent, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "cmpl_parent",
            "inst-parent",
            "create-parent",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event(
            "inst-parent",
            "open",
            Value::Obj(BTreeMap::new()),
            "open-1",
            None,
        )
        .unwrap();
    // Entering the invoking state arms the slot; starting it is the record
    // that brings the child into existence.
    store.invoke_child("inst-parent", "down", "inv-1").unwrap();

    let offered = values(&ask("fsm://instance/{id}", "id", "", &store));
    assert_eq!(offered.len(), 2, "parent and child: {offered:?}");
    assert_eq!(
        offered[1], "inst-parent",
        "the child was created later, so it is offered first: {offered:?}"
    );
    // And the child's id is one `resources/read` resolves.
    resources::read(&format!("fsm://instance/{}", offered[0]), Some(&store)).unwrap();
}

#[test]
fn two_hundred_and_fifty_instances_truncate_and_say_so() {
    let dir = scratch("many");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, machine("cmpl_many"), false, false)
        .unwrap();
    for n in 0..250 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "cmpl_many",
                &format!("inst-{n:03}"),
                &format!("create-{n}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let result = ask("fsm://instance/{id}", "id", "", &store);
    assert_eq!(values(&result).len(), 100);
    assert_eq!(total(&result), 250);
    assert!(has_more(&result));
}

#[test]
fn a_completed_value_reads_back() {
    // A completion that yields an unreadable URI is worse than none.
    let dir = scratch("roundtrip");
    let (store, _) = five_machines(&dir);
    for id in values(&ask("fsm://machine/{id}", "id", "", &store)) {
        resources::read(&format!("fsm://machine/{id}"), Some(&store))
            .unwrap_or_else(|e| panic!("{id} does not read back: {e:?}"));
    }
}

#[test]
fn a_machine_name_is_never_offered_under_an_id() {
    let dir = scratch("names");
    let (store, _) = five_machines(&dir);
    let offered = values(&ask("fsm://machine/{id}", "id", "", &store));
    for name in ["cmpl_0", "cmpl_1", "cmpl_2", "cmpl_3", "cmpl_4"] {
        assert!(
            !offered.iter().any(|id| id == name),
            "a name under an id argument composes into a URI that fails to read"
        );
    }
}

#[test]
fn a_variable_this_task_does_not_serve_is_answered_empty() {
    let dir = scratch("unknown");
    let (store, _) = five_machines(&dir);
    let result = ask("fsm://machine/{id}", "flavour", "", &store);
    assert!(values(&result).is_empty());
    assert_eq!(total(&result), 0);
    let result = ask("fsm://docs/{name}", "name", "", &store);
    assert!(values(&result).is_empty());
}

#[test]
fn the_completion_and_the_listing_agree_about_order() {
    let dir = scratch("agree");
    let mut store = five_machines(&dir).0;
    let mut clock = FixedClock::new(2_000, 1);
    for n in 0..3 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "cmpl_0",
                &format!("inst-{n}"),
                &format!("create-{n}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let listing = resources::list(Some(&store));
    let listed: Vec<String> = listing
        .get("resources")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("uri").and_then(Value::as_str))
        .filter(|uri| uri.starts_with("fsm://instance/"))
        .map(|uri| uri.trim_start_matches("fsm://instance/").to_string())
        .collect();
    assert_eq!(
        values(&ask("fsm://instance/{id}", "id", "", &store)),
        listed,
        "one ordering rule, one implementation"
    );

    let listed_machines: Vec<String> = listing
        .get("resources")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("uri").and_then(Value::as_str))
        .filter(|uri| uri.starts_with("fsm://machine/"))
        .map(|uri| uri.trim_start_matches("fsm://machine/").to_string())
        .collect();
    assert_eq!(
        values(&ask("fsm://machine/{id}", "id", "", &store)),
        listed_machines
    );
}

#[test]
fn a_completion_never_walks_the_journal() {
    // Proved by taking the journal away. Every id and every ordering here
    // comes from folded state and the store's own indexes, so emptying the
    // record vector changes nothing — where a supplier that scanned records
    // would answer differently, or not at all.
    let dir = scratch("nowalk");
    let (mut store, _) = five_machines(&dir);
    let mut clock = FixedClock::new(2_000, 1);
    for n in 0..5 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "cmpl_0",
                &format!("inst-{n}"),
                &format!("create-{n}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let machines = values(&ask("fsm://machine/{id}", "id", "", &store));
    let instances = values(&ask("fsm://instance/{id}", "id", "", &store));
    assert_eq!(instances.len(), 5);

    store.records.clear();
    assert_eq!(
        values(&ask("fsm://machine/{id}", "id", "", &store)),
        machines,
        "the machine completion read the journal"
    );
    assert_eq!(
        values(&ask("fsm://instance/{id}", "id", "", &store)),
        instances,
        "the instance completion read the journal"
    );
}
