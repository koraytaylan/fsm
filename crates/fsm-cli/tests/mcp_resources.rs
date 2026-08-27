//! Instances are live objects, so they have URIs — and a resource and a tool
//! must never disagree about what one looks like.
//!
//! Plan 0012 task 5801.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::resources;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Rendered views are counted per process, so the test that asserts a
/// listing renders none needs the other tests in this binary to hold still
/// while it looks. Every test takes it; they are all fast.
static VIEWS: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-res-{}-{n}", std::process::id()));
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

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"res_case","states":[{"name":"intake"},{"name":"done","terminal":true}],"initial":"intake","context":[{"name":"score","ty":"int","init":"1"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"done"}]}"#;

const CHILD: &str = r#"{"format":"fsm.machine/1","name":"res_child","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

fn parent_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"res_parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.down","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#,
        digest(CHILD)
    )
}

/// A store with one plain instance.
fn one_instance(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "res_case",
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    store
}

fn body_of(read: &Value) -> Value {
    let text = read
        .get("contents")
        .and_then(Value::as_arr)
        .and_then(|contents| contents.first())
        .and_then(|entry| entry.get("text"))
        .and_then(Value::as_str)
        .expect("one content entry with text");
    parse(text.as_bytes(), &JsonLimits::DEFAULT).expect("the body is JSON")
}

#[test]
fn an_instance_resource_says_exactly_what_the_tool_says() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let mut store = one_instance(&directory);
    let mut clock = FixedClock::new(2_000, 1);
    let from_tool = dispatch(
        &mut store,
        &mut clock,
        "instance_get",
        &obj(&[("instance_id", text("inst-1"))]),
    )
    .unwrap();
    let from_resource = body_of(&resources::read("fsm://instance/inst-1", Some(&store)).unwrap());
    assert_eq!(
        from_resource, from_tool,
        "a resource and a tool disagreeing about an instance is the failure this prevents"
    );
    assert_eq!(
        resources::read("fsm://instance/inst-1", Some(&store))
            .unwrap()
            .get("contents")
            .and_then(Value::as_arr)
            .and_then(|c| c.first())
            .and_then(|entry| entry.get("mimeType"))
            .and_then(Value::as_str),
        Some("application/json")
    );
}

#[test]
fn the_history_resource_is_a_bounded_page() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let mut store = one_instance(&directory);
    let mut clock = FixedClock::new(2_000, 1);
    let from_tool = dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &obj(&[("instance_id", text("inst-1"))]),
    )
    .unwrap();
    let from_resource =
        body_of(&resources::read("fsm://instance/inst-1/history", Some(&store)).unwrap());
    assert_eq!(
        from_resource
            .get("entries")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        from_tool
            .get("entries")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        "the resource is the tool's default page"
    );
}

#[test]
fn both_templates_are_listed_with_their_mime_types() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let listed = resources::templates();
    let templates = listed
        .get("resourceTemplates")
        .and_then(Value::as_arr)
        .unwrap();
    let uris: Vec<&str> = templates
        .iter()
        .filter_map(|entry| entry.get("uriTemplate").and_then(Value::as_str))
        .collect();
    assert_eq!(
        uris,
        [
            "fsm://machine/{id}",
            "fsm://instance/{id}",
            "fsm://instance/{id}/history"
        ]
    );
    for entry in templates {
        assert_eq!(
            entry.get("mimeType").and_then(Value::as_str),
            Some("application/json")
        );
        assert!(entry.get("title").is_some());
        assert!(entry.get("name").is_some());
    }
    // The history template says where to page.
    let history = templates.last().unwrap();
    assert!(
        history
            .get("description")
            .and_then(Value::as_str)
            .is_some_and(|d| d.contains("instance_history")),
        "a resource that could return an unbounded journal has to say so"
    );
}

#[test]
fn the_listing_is_most_recent_first_and_capped() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    for index in 0..60 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "res_case",
                &format!("inst-{index:02}"),
                &format!("create-{index}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let listed = resources::list(Some(&store));
    let uris: Vec<&str> = listed
        .get("resources")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("uri").and_then(Value::as_str))
        .filter(|uri| uri.starts_with("fsm://instance/"))
        .collect();
    assert_eq!(uris.len(), 50, "the cap holds with sixty present");
    assert_eq!(uris[0], "fsm://instance/inst-59", "most recent first");
    assert_eq!(uris[49], "fsm://instance/inst-10");
}

#[test]
fn a_child_instance_is_listed_and_ordered_with_the_rest() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    // The case an `instance_created` scan would have dropped: a child has no
    // creation record of its own.
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for source in [CHILD.to_string(), parent_source()] {
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            "res_parent",
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
    store.invoke_child("p1", "down", "inv-1").unwrap();
    let child = child_instance_id("p1", "down");

    let listed = resources::list(Some(&store));
    let uris: Vec<&str> = listed
        .get("resources")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("uri").and_then(Value::as_str))
        .filter(|uri| uri.starts_with("fsm://instance/"))
        .collect();
    assert_eq!(
        uris,
        [
            format!("fsm://instance/{child}"),
            "fsm://instance/p1".to_string()
        ],
        "the child exists, and it appeared after its parent"
    );
    // And it reads.
    let body = body_of(&resources::read(&format!("fsm://instance/{child}"), Some(&store)).unwrap());
    assert_eq!(
        body.get("instance_id").and_then(Value::as_str),
        Some(child.as_str())
    );
}

#[test]
fn everything_unknown_is_the_one_not_found() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let store = one_instance(&directory);
    for uri in [
        "fsm://instance/nosuch",
        "fsm://instance/",
        "fsm://instance/inst-1/ledger",
        "fsm://instance/inst-1/history/2",
        "fsm://nonsense",
    ] {
        let error = resources::read(uri, Some(&store)).expect_err(uri);
        assert_eq!(error.code, "mcp/resource_not_found", "{uri}");
    }
    // And with no store at all.
    let error = resources::read("fsm://instance/inst-1", None).expect_err("no store");
    assert_eq!(error.code, "mcp/resource_not_found");
}

#[test]
fn the_documentation_and_machine_resources_did_not_move() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let store = one_instance(&directory);
    let listed = resources::list(Some(&store));
    let entries = listed.get("resources").and_then(Value::as_arr).unwrap();
    assert_eq!(
        entries[0].get("uri").and_then(Value::as_str),
        Some("fsm://docs/spec")
    );
    assert_eq!(
        entries[1].get("uri").and_then(Value::as_str),
        Some("fsm://docs/examples")
    );
    assert!(
        entries[2]
            .get("uri")
            .and_then(Value::as_str)
            .is_some_and(|uri| uri.starts_with("fsm://machine/")),
        "the machine entries still come before the instances"
    );
    let spec = resources::read("fsm://docs/spec", Some(&store)).unwrap();
    assert_eq!(
        spec.get("contents")
            .and_then(Value::as_arr)
            .and_then(|c| c.first())
            .and_then(|entry| entry.get("mimeType"))
            .and_then(Value::as_str),
        Some("text/markdown")
    );
}

#[test]
fn a_store_with_no_instances_lists_only_docs_and_machines() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    store
        .define_machine_on(&mut FixedClock::new(1_000, 1), value(CASE), false, false)
        .unwrap();
    let listed = resources::list(Some(&store));
    let uris: Vec<&str> = listed
        .get("resources")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("uri").and_then(Value::as_str))
        .collect();
    assert_eq!(uris.len(), 3, "{uris:?}");
    assert!(!uris.iter().any(|uri| uri.starts_with("fsm://instance/")));
}

#[test]
fn a_settled_instance_reads_as_settled() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let mut store = one_instance(&directory);
    store.cancel_instance("inst-1", "cancel-1").unwrap();
    let body = body_of(&resources::read("fsm://instance/inst-1", Some(&store)).unwrap());
    assert_eq!(
        body.get("status").and_then(Value::as_str),
        Some("cancelled")
    );
}

// ---------------------------------------------------------------------------
// Titles: `name` identifies, `title` is read. Plan 0013 task 6202.
// ---------------------------------------------------------------------------

fn entries(listing: &Value) -> Vec<Value> {
    listing
        .get("resources")
        .and_then(Value::as_arr)
        .expect("resources")
        .to_vec()
}

#[test]
fn every_listed_resource_and_template_carries_a_title() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let store = one_instance(&directory);
    for entry in entries(&resources::list(Some(&store))) {
        let uri = entry.get("uri").and_then(Value::as_str).unwrap();
        for field in ["name", "title"] {
            let got = entry.get(field).and_then(Value::as_str).unwrap_or("");
            assert!(!got.is_empty(), "{uri} has no {field}");
        }
    }
    let templates = resources::templates();
    for entry in templates
        .get("resourceTemplates")
        .and_then(Value::as_arr)
        .unwrap()
    {
        let uri = entry.get("uriTemplate").and_then(Value::as_str).unwrap();
        let title = entry.get("title").and_then(Value::as_str).unwrap_or("");
        assert!(!title.is_empty(), "{uri} has no title");
        assert_ne!(
            title, uri,
            "a title that restates the URI tells a reader nothing"
        );
    }
}

#[test]
fn the_documentation_resources_keep_the_names_clients_key_on() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let store = one_instance(&directory);
    let listed = entries(&resources::list(Some(&store)));
    let by_uri = |uri: &str| {
        listed
            .iter()
            .find(|e| e.get("uri").and_then(Value::as_str) == Some(uri))
            .cloned()
            .unwrap_or_else(|| panic!("{uri} missing"))
    };
    assert_eq!(
        by_uri("fsm://docs/spec")
            .get("name")
            .and_then(Value::as_str),
        Some("Machine spec & expression reference"),
        "this task adds a field and changes no name"
    );
    assert_eq!(
        by_uri("fsm://docs/examples")
            .get("name")
            .and_then(Value::as_str),
        Some("Worked examples")
    );
    assert_eq!(
        by_uri("fsm://instance/inst-1")
            .get("name")
            .and_then(Value::as_str),
        Some("inst-1")
    );
}

#[test]
fn a_machine_is_named_by_its_identifier_and_titled_by_its_name() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let store = one_instance(&directory);
    let entry = entries(&resources::list(Some(&store)))
        .into_iter()
        .find(|e| {
            e.get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| uri.starts_with("fsm://machine/"))
        })
        .expect("the machine is listed");
    let uri = entry.get("uri").and_then(Value::as_str).unwrap();
    let id = uri.trim_start_matches("fsm://machine/");
    assert_eq!(entry.get("name").and_then(Value::as_str), Some(id));
    assert_eq!(entry.get("title").and_then(Value::as_str), Some("res_case"));
    assert_ne!(
        entry.get("name"),
        entry.get("title"),
        "a machine's identifier and the name somebody wrote are different facts"
    );
}

#[test]
fn an_instance_is_titled_by_its_machine_and_where_it_is() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let directory = TestDirectory::create();
    let mut store = one_instance(&directory);
    let title_now = || {
        entries(&resources::list(Some(&store)))
            .into_iter()
            .find(|e| e.get("uri").and_then(Value::as_str) == Some("fsm://instance/inst-1"))
            .and_then(|e| e.get("title").and_then(Value::as_str).map(str::to_string))
            .unwrap()
    };
    // The helper has already driven this instance to its terminal state.
    assert_eq!(
        title_now(),
        "res_case — done",
        "a listing is readable at a glance or it is a list of opaque ids"
    );

    // And a fresh instance is titled where *it* is: the title follows the
    // instance rather than being stamped once at creation.
    let mut clock = FixedClock::new(3_000, 1);
    store
        .create_instance_ctx_on(
            &mut clock,
            "res_case",
            "inst-2",
            "create-2",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let fresh = entries(&resources::list(Some(&store)))
        .into_iter()
        .find(|e| e.get("uri").and_then(Value::as_str) == Some("fsm://instance/inst-2"))
        .unwrap();
    assert_eq!(
        fresh.get("title").and_then(Value::as_str),
        Some("res_case — intake")
    );
}

#[test]
fn a_listing_renders_no_instance_views() {
    let _views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    // The expensive read in this store is a view: it scans enabled events,
    // which evaluates every guard leaving the configuration. A title is worth
    // a map lookup and a leaf read; it is never worth one of those per row.
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    for n in 0..60 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "res_case",
                &format!("inst-{n}"),
                &format!("create-{n}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let before = fsm_cli::store::views_rendered();
    let listed = entries(&resources::list(Some(&store)));
    let after = fsm_cli::store::views_rendered();
    assert!(listed.len() >= 50, "the instances are listed");
    assert_eq!(
        after,
        before,
        "listing 60 instances rendered {} views",
        after - before
    );

    // One read of one instance renders exactly one, so the counter is live
    // rather than broken.
    let before = fsm_cli::store::views_rendered();
    resources::read("fsm://instance/inst-1", Some(&store)).unwrap();
    assert_eq!(fsm_cli::store::views_rendered(), before + 1);
}
