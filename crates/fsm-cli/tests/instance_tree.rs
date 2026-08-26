//! Once instances have parents, a flat listing is a store nobody can
//! navigate: the surface has to show the edges.
//!
//! Plan 0010 task 5103.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{dispatch, registry, validate_args};
use fsm_cli::store::Store;
use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-tree-{}-{n}", std::process::id()));
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

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(src: &str) -> String {
    digest_of(&machine_id(&value(src))).unwrap().to_string()
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect(),
    )
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

const LEAF: &str = r#"{"format":"fsm.machine/1","name":"leaf","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

/// A waiter that invokes `child_digest` from `busy`, with another way out so
/// it carries no wait-forever warning.
fn waiter(name: &str, child_digest: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{child_digest}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.down","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#
    )
}

/// A tree `depth` levels deep, every slot invoked. Returns ids, root first.
fn tree(directory: &TestDirectory, depth: usize) -> (Store, Vec<String>) {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(LEAF), false, false)
        .unwrap();
    let mut below = digest(LEAF);
    let mut root = String::new();
    for level in 0..depth {
        let name = format!("waiter{level}");
        let source = waiter(&name, &below);
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
        below = digest(&source);
        root = name;
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            &root,
            "root",
            "c-root",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let mut ids = vec!["root".to_string()];
    let mut current = "root".to_string();
    for level in 0..depth {
        store
            .send_event(
                &current,
                "open",
                Value::Obj(BTreeMap::new()),
                &format!("open-{level}"),
                None,
            )
            .unwrap();
        store
            .invoke_child(&current, "down", &format!("inv-{level}"))
            .unwrap();
        current = child_instance_id(&current, "down");
        ids.push(current.clone());
    }
    (store, ids)
}

fn get(store: &mut Store, clock: &mut FixedClock, instance_id: &str) -> Value {
    dispatch(
        store,
        clock,
        "instance_get",
        &obj(&[("instance_id", text(instance_id))]),
    )
    .unwrap()
}

#[test]
fn a_child_names_its_parent_and_a_parent_names_its_children() {
    let directory = TestDirectory::create();
    let (mut store, ids) = tree(&directory, 1);
    let mut clock = FixedClock::new(2_000, 1);
    let child = ids[1].clone();

    let view = get(&mut store, &mut clock, &child);
    let parent = view.get("parent").expect("a child reports its parent");
    assert_eq!(
        parent.get("instance_id").and_then(Value::as_str),
        Some("root")
    );
    assert_eq!(parent.get("slot").and_then(Value::as_str), Some("down"));
    assert!(
        view.get("children")
            .and_then(Value::as_arr)
            .unwrap()
            .is_empty(),
        "the leaf invokes nothing"
    );

    let view = get(&mut store, &mut clock, "root");
    assert_eq!(
        view.get("parent"),
        Some(&Value::Null),
        "a root has no parent"
    );
    let children = view.get("children").and_then(Value::as_arr).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(
        children[0].get("child_instance_id").and_then(Value::as_str),
        Some(child.as_str())
    );
    assert_eq!(
        children[0].get("slot").and_then(Value::as_str),
        Some("down")
    );
    assert_eq!(
        children[0].get("status").and_then(Value::as_str),
        Some("running")
    );
    assert_eq!(
        children[0].get("invocation_status").and_then(Value::as_str),
        Some("running")
    );
}

#[test]
fn created_seq_answers_for_children_too_and_survives_a_refold() {
    let directory = TestDirectory::create();
    let (mut store, ids) = tree(&directory, 1);
    let mut clock = FixedClock::new(2_000, 1);
    let seqs: Vec<u64> = ids
        .iter()
        .map(|id| {
            get(&mut store, &mut clock, id)
                .get("created_seq")
                .and_then(Value::as_num)
                .and_then(|n| n.parse::<u64>().ok())
                .expect("every instance reports one")
        })
        .collect();
    assert!(seqs[0] < seqs[1], "the child appeared after its parent");
    // The child's creation record is the invocation, which is what makes the
    // answer uniform: a scan for `instance_created` would find nothing.
    let record = &store.records[seqs[1] as usize];
    assert_eq!(record.kind, fsm_core::record::RecordKind::InstanceInvoked);
    assert_eq!(
        store.records[seqs[0] as usize].kind,
        fsm_core::record::RecordKind::InstanceCreated
    );
    drop(store);
    let mut reopened = Store::open(directory.path()).unwrap();
    let after: Vec<u64> = ids
        .iter()
        .map(|id| {
            get(&mut reopened, &mut clock, id)
                .get("created_seq")
                .and_then(Value::as_num)
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap()
        })
        .collect();
    assert_eq!(after, seqs, "the answer is stable across a re-fold");
}

#[test]
fn the_listing_filters_one_tree_or_every_root() {
    let directory = TestDirectory::create();
    let (mut store, ids) = tree(&directory, 2);
    let mut clock = FixedClock::new(2_000, 1);
    let names = |value: &Value| -> Vec<String> {
        value
            .get("instances")
            .and_then(Value::as_arr)
            .unwrap()
            .iter()
            .filter_map(|row| row.get("instance_id").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    };

    let all = dispatch(&mut store, &mut clock, "instance_list", &obj(&[])).unwrap();
    assert_eq!(
        names(&all).len(),
        3,
        "a flat listing still lists everything"
    );

    let children = dispatch(
        &mut store,
        &mut clock,
        "instance_list",
        &obj(&[("parent", text("root"))]),
    )
    .unwrap();
    assert_eq!(names(&children), vec![ids[1].clone()]);

    let roots = dispatch(
        &mut store,
        &mut clock,
        "instance_list",
        &obj(&[("roots_only", Value::Bool(true))]),
    )
    .unwrap();
    assert_eq!(names(&roots), vec!["root".to_string()]);

    // And every row carries the tree fields.
    let rows = all.get("instances").and_then(Value::as_arr).unwrap();
    for row in rows {
        assert!(row.get("created_seq").is_some());
        let id = row.get("instance_id").and_then(Value::as_str).unwrap();
        assert_eq!(row.get("parent").is_some(), id != "root", "{id}");
    }
}

#[test]
fn a_filter_composes_with_the_cursor_across_two_pages() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    // One parent with two slots, so a page boundary falls inside one tree.
    store
        .define_machine_on(&mut clock, value(LEAF), false, false)
        .unwrap();
    let two_slots = format!(
        r#"{{"format":"fsm.machine/1","name":"two","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"a","machine":"{d}"}},{{"id":"b","machine":"{d}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.a","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#,
        d = digest(LEAF)
    );
    store
        .define_machine_on(&mut clock, value(&two_slots), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(&mut clock, "two", "root", "c1", None, &BTreeMap::new(), &[])
        .unwrap();
    store
        .send_event("root", "open", Value::Obj(BTreeMap::new()), "open-1", None)
        .unwrap();
    for slot in ["a", "b"] {
        store
            .invoke_child("root", slot, &format!("inv-{slot}"))
            .unwrap();
    }

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut args = vec![("parent", text("root")), ("limit", Value::Num("1".into()))];
        if let Some(c) = &cursor {
            args.push(("cursor", text(c)));
        }
        let page = dispatch(&mut store, &mut clock, "instance_list", &obj(&args)).unwrap();
        let rows = page.get("instances").and_then(Value::as_arr).unwrap();
        seen.extend(
            rows.iter()
                .filter_map(|row| row.get("instance_id").and_then(Value::as_str))
                .map(str::to_string),
        );
        match page.get("next_cursor").and_then(Value::as_str) {
            Some(next) => cursor = Some(next.to_string()),
            None => break,
        }
    }
    let mut expected = vec![
        child_instance_id("root", "a"),
        child_instance_id("root", "b"),
    ];
    expected.sort();
    seen.sort();
    assert_eq!(seen, expected, "both children, one per page, filter intact");
}

#[test]
fn a_depth_three_tree_is_navigable_from_the_root() {
    let directory = TestDirectory::create();
    let (mut store, ids) = tree(&directory, 3);
    let mut clock = FixedClock::new(2_000, 1);
    let mut walked = vec!["root".to_string()];
    let mut current = "root".to_string();
    while let Some(next) = get(&mut store, &mut clock, &current)
        .get("children")
        .and_then(Value::as_arr)
        .and_then(|children| children.first().cloned())
        .and_then(|child| {
            child
                .get("child_instance_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    {
        walked.push(next.clone());
        current = next;
    }
    assert_eq!(walked, ids, "every level reachable with instance_get alone");
}

#[test]
fn the_view_validates_against_its_declared_schema() {
    let directory = TestDirectory::create();
    let (mut store, ids) = tree(&directory, 1);
    let mut clock = FixedClock::new(2_000, 1);
    let spec = registry()
        .into_iter()
        .find(|tool| tool.name == "instance_get")
        .unwrap();
    for id in &ids {
        let view = get(&mut store, &mut clock, id);
        validate_args(&(spec.output_schema)(), &view).unwrap_or_else(|e| panic!("{id}: {e:?}"));
    }
}

#[test]
fn a_slot_is_visible_in_both_diagram_formats() {
    let source = waiter("drawn", &digest(LEAF));
    let spec = fsm_core::spec::parse_machine(&value(&source)).unwrap();
    let compiled = fsm_core::spec::compile(spec).unwrap();
    let short = &digest(LEAF)[..8];
    let mermaid = fsm_core::diagram::mermaid(&compiled, None);
    assert!(
        mermaid.contains(&format!("<<invoke down → {short}>>")),
        "{mermaid}"
    );
    let dot = fsm_core::diagram::dot(&compiled, None);
    assert!(dot.contains("shape=box3d"), "{dot}");
    assert!(dot.contains(&format!("down · {short}")), "{dot}");
}

#[test]
fn two_slots_on_one_state_are_distinguishable() {
    let second = r#"{"format":"fsm.machine/1","name":"other","states":[{"name":"w"}],"initial":"w","context":[],"events":[],"transitions":[]}"#;
    let source = format!(
        r#"{{"format":"fsm.machine/1","name":"pair","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"a","machine":"{first}"}},{{"id":"b","machine":"{second_digest}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.a","to":"out"}},{{"from":"busy","on":"$done.invoke.b","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#,
        first = digest(LEAF),
        second_digest = digest(second)
    );
    let spec = fsm_core::spec::parse_machine(&value(&source)).unwrap();
    let compiled = fsm_core::spec::compile(spec).unwrap();
    let mermaid = fsm_core::diagram::mermaid(&compiled, None);
    assert!(mermaid.contains(&digest(LEAF)[..8]) && mermaid.contains(&digest(second)[..8]));
    let dot = fsm_core::diagram::dot(&compiled, None);
    assert_eq!(dot.matches("shape=box3d").count(), 2);
}

#[test]
fn analysis_reports_both_composition_smells_and_neither_when_well_formed() {
    let leaf_digest = digest(LEAF);
    let cases: &[(&str, &[&str])] = &[
        // A slot nothing handles, on a state with another way out.
        (
            r#"[{"from":"idle","on":"open","to":"busy"},{"from":"busy","on":"give_up","to":"out"}]"#,
            &["def/invoke_result_unhandled"],
        ),
        // Handled, but the result is the only way out.
        (
            r#"[{"from":"idle","on":"open","to":"busy"},{"from":"busy","on":"$done.invoke.down","to":"out"}]"#,
            &["def/invoke_only_exit"],
        ),
        // Handled, with another exit: neither warning.
        (
            r#"[{"from":"idle","on":"open","to":"busy"},{"from":"busy","on":"$done.invoke.down","to":"out"},{"from":"busy","on":"give_up","to":"out"}]"#,
            &[],
        ),
    ];
    for (transitions, expected) in cases {
        let source = format!(
            r#"{{"format":"fsm.machine/1","name":"smell","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{leaf_digest}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":{transitions}}}"#
        );
        let spec = fsm_core::spec::parse_machine(&value(&source)).unwrap();
        let compiled = fsm_core::spec::compile(spec).unwrap();
        let codes: Vec<&str> = fsm_core::analyze::invoke_findings(&compiled)
            .iter()
            .map(|finding| finding.code)
            .collect();
        assert_eq!(&codes, expected, "{transitions}");
    }
}
