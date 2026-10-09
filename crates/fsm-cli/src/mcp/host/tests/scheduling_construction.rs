//! Production mode selection must leave genuine configured handlers unstarted.
//! Disposable Root observation separately proves that no domain was allocated.

use std::{fs, io::Cursor, path::PathBuf};

use fsm_execute::config::HandlerTable;

use crate::mcp::{
    notify::SharedSink,
    serve::{ExecutorLoop, ServeMode, serve_dir_with},
};

use super::{FixedClock, Store, Value, held_handlers, value};

const INPUT: &str = concat!(
    "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
    "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"resources/read\",\"params\":{\"uri\":\"fsm://executor\"}}\n"
);

fn executor(path: &std::path::Path) -> ExecutorLoop {
    let handlers =
        HandlerTable::parse(&fs::read_to_string(path.join("handlers.json")).unwrap()).unwrap();
    ExecutorLoop::new(path, handlers).unwrap()
}

fn observe(path: &std::path::Path, mode: ServeMode, expected: &str) {
    let output = SharedSink::new();
    serve_dir_with(path, mode, Cursor::new(INPUT), output.writer()).unwrap();
    let frames = output.text().lines().map(value).collect::<Vec<_>>();
    let initialized = frames
        .iter()
        .find(|frame| frame.get("id") == Some(&value("1")))
        .unwrap();
    assert!(initialized.get("result").is_some());
    let discovered = frames
        .iter()
        .find(|frame| frame.get("id") == Some(&value("2")))
        .unwrap();
    let content = discovered
        .get("result")
        .unwrap()
        .get("contents")
        .and_then(Value::as_arr)
        .unwrap();
    let report = value(content[0].get("text").and_then(Value::as_str).unwrap());
    assert_eq!(report.get("mode").and_then(Value::as_str), Some(expected));
    assert_eq!(report.get("executes_effects"), Some(&Value::Bool(false)));
    assert_eq!(
        report.get("handlers"),
        Some(&if expected == "writer" {
            value("[]")
        } else {
            Value::Null
        })
    );
}

#[test]
#[ignore = "requires disposable native CI and independent zero-allocation authority observation"]
fn autonomous_schedule_restricted_modes_start_no_genuine_fixture() {
    let manifest = held_handlers::manifest();
    assert_eq!(
        held_handlers::field(&manifest, "behavior"),
        "schedule-construction"
    );
    let path = PathBuf::from(held_handlers::field(&manifest, "store"));
    let resource = PathBuf::from(held_handlers::field(&manifest, "resource"));
    let mut store = Store::open(&path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    store
        .define_machine_on(&mut clock, value(held_handlers::HELD_MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "held_completion",
            "held",
            "create-held",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    let original = store.records.clone();
    drop(store);
    observe(&path, ServeMode::Writer, "writer");
    observe(&path, ServeMode::ReadOnly, "read-only");
    let held = Store::open(&path).unwrap();
    observe(
        &path,
        ServeMode::Embedded(Box::new(executor(&path))),
        "read-only",
    );
    assert_eq!(held.records, original);
    drop(held);
    // A file where a data directory is required exercises the production open
    // failure path without altering the healthy original journal fixture.
    let unavailable = path.join("unavailable-store");
    fs::write(&unavailable, b"not a directory").unwrap();
    observe(
        &unavailable,
        ServeMode::Embedded(Box::new(executor(&path))),
        "degraded",
    );
    let reopened = Store::open_read_only(&path).unwrap();
    assert_eq!(reopened.records, original);
    assert_eq!(reopened.state.instances["held"].pending.len(), 1);
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    for role in ["root", "child", "grandchild"] {
        assert_eq!(
            fs::read(resource.join(format!("{role}-entered"))).unwrap(),
            b""
        );
        assert!(!resource.join(format!("{role}-candidate")).exists());
    }
    assert_eq!(
        crate::journal_io::verify(&path).health,
        crate::journal_io::JournalHealth::Ok
    );
}
