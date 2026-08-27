//! The driving prompts, and the completion that earns the capability: an
//! `event` argument answered from the named instance's own analysis.
//!
//! Plan 0013 task 6303.

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::complete::complete;
use fsm_cli::mcp::prompts::{get, list};
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
        "fsm-cmplp-{tag}-{}-{}",
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

/// One event fires now, one is internal, one is guarded off, and one only
/// fires depending on its payload.
const CASE: &str = r#"{"format":"fsm.machine/1","name":"drive_case","states":[{"name":"open"},{"name":"held"},{"name":"shut","terminal":true}],"initial":"open","context":[{"name":"ready","ty":"bool","init":"false"}],"events":[{"name":"push","fields":[]},{"name":"shove","fields":[{"name":"force","ty":"int"}]},{"name":"seal","fields":[]},{"name":"tick","fields":[],"internal":true}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"open","on":"shove","to":"held","if":"evt.force > 5"},{"from":"open","on":"seal","to":"shut","if":"ctx.ready"},{"from":"open","on":"tick","to":"held"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    for n in 0..3 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "drive_case",
                &format!("inst-{n}"),
                &format!("create-{n}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    store
}

fn ask(prompt: &str, argument: &str, prefix: &str, context: &str, store: &Store) -> Value {
    let context = if context.is_empty() {
        String::new()
    } else {
        format!(r#","context":{{"arguments":{{"instance_id":"{context}"}}}}"#)
    };
    let request = value(&format!(
        r#"{{"ref":{{"type":"ref/prompt","name":"{prompt}"}},"argument":{{"name":"{argument}","value":"{prefix}"}}{context}}}"#
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

fn prompts() -> Vec<Value> {
    list()
        .get("prompts")
        .and_then(Value::as_arr)
        .unwrap()
        .to_vec()
}

fn text_of(result: &Value) -> String {
    result
        .get("messages")
        .and_then(Value::as_arr)
        .and_then(|m| m.first())
        .and_then(|m| m.get("content"))
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap()
        .to_string()
}

#[test]
fn three_prompts_with_their_arguments() {
    let listed = prompts();
    let names: Vec<&str> = listed
        .iter()
        .filter_map(|p| p.get("name").and_then(Value::as_str))
        .collect();
    assert_eq!(
        names,
        ["author_machine", "drive_instance", "diagnose_instance"]
    );

    let arguments = |prompt: &str| -> Vec<(String, bool)> {
        listed
            .iter()
            .find(|p| p.get("name").and_then(Value::as_str) == Some(prompt))
            .and_then(|p| p.get("arguments"))
            .and_then(Value::as_arr)
            .unwrap()
            .iter()
            .map(|a| {
                (
                    a.get("name").and_then(Value::as_str).unwrap().to_string(),
                    a.get("required").and_then(Value::as_bool).unwrap(),
                )
            })
            .collect()
    };
    assert_eq!(arguments("author_machine"), [("goal".to_string(), true)]);
    assert_eq!(
        arguments("drive_instance"),
        [
            ("instance_id".to_string(), true),
            ("event".to_string(), false)
        ]
    );
    assert_eq!(
        arguments("diagnose_instance"),
        [("instance_id".to_string(), true)]
    );
    for prompt in &listed {
        let name = prompt.get("name").and_then(Value::as_str).unwrap();
        assert!(
            prompt
                .get("title")
                .and_then(Value::as_str)
                .is_some_and(|t| !t.is_empty()),
            "{name} has no title"
        );
    }
}

#[test]
fn each_prompt_is_a_route_through_the_surface() {
    let driving = text_of(
        &get(
            "drive_instance",
            Some(&value(r#"{"instance_id":"inst-1"}"#)),
        )
        .unwrap(),
    );
    for needle in [
        "instance_get",
        "enabled_events",
        "deadlines_pending",
        "effect_ack",
        "request_id",
        "fsm://instance/inst-1",
    ] {
        assert!(driving.contains(needle), "drive_instance omits {needle}");
    }

    let diagnosing = text_of(
        &get(
            "diagnose_instance",
            Some(&value(r#"{"instance_id":"inst-1"}"#)),
        )
        .unwrap(),
    );
    for needle in [
        "instance_history",
        "trace",
        "Explain",
        "fsm://instance/inst-1/history",
    ] {
        assert!(
            diagnosing.contains(needle),
            "diagnose_instance omits {needle}"
        );
    }

    // And a missing required argument names the field.
    let error = get("drive_instance", None).unwrap_err();
    assert_eq!(
        error.details.get("field").and_then(Value::as_str),
        Some("instance_id")
    );
    let error = get("nope", None).unwrap_err();
    assert!(error.hint.contains("drive_instance"), "{}", error.hint);
}

#[test]
fn an_instance_id_completes_on_both_driving_prompts() {
    let dir = scratch("ids");
    let store = seeded(&dir);
    for prompt in ["drive_instance", "diagnose_instance"] {
        assert_eq!(
            values(&ask(prompt, "instance_id", "", "", &store)),
            ["inst-2", "inst-1", "inst-0"],
            "{prompt}"
        );
    }
    assert_eq!(
        values(&ask("drive_instance", "instance_id", "inst-1", "", &store)),
        ["inst-1"]
    );
}

#[test]
fn an_event_completes_from_the_named_instances_own_analysis() {
    let dir = scratch("events");
    let store = seeded(&dir);
    let offered = values(&ask("drive_instance", "event", "", "inst-0", &store));

    // Exactly what `instance_get` reports as sendable for this instance.
    let view = store.instance_report("inst-0").unwrap();
    let expected: Vec<String> = view
        .get("enabled_events")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter(|e| {
            matches!(
                e.get("status").and_then(Value::as_str),
                Some("enabled" | "depends_on_payload")
            )
        })
        .map(|e| e.get("event").and_then(Value::as_str).unwrap().to_string())
        .collect();
    assert_eq!(offered, expected);
    assert!(offered.contains(&"push".to_string()), "{offered:?}");

    // A prefix narrows it like every other completion.
    assert_eq!(
        values(&ask("drive_instance", "event", "pu", "inst-0", &store)),
        ["push"]
    );
}

#[test]
fn an_event_nobody_outside_may_send_is_never_offered() {
    let dir = scratch("internal");
    let store = seeded(&dir);
    let offered = values(&ask("drive_instance", "event", "", "inst-0", &store));
    assert!(
        !offered.iter().any(|e| e == "tick"),
        "an internal event is not a suggestion: {offered:?}"
    );
    assert!(
        !offered.iter().any(|e| e.starts_with('$')),
        "a generated event is not a suggestion: {offered:?}"
    );
    assert!(
        !offered.iter().any(|e| e == "seal"),
        "a guard that is false now is not a suggestion: {offered:?}"
    );
}

#[test]
fn an_event_without_the_instance_is_answered_empty() {
    // Guessing from the catalogue would suggest events that cannot fire
    // against this instance, which is worse than offering nothing.
    let dir = scratch("nocontext");
    let store = seeded(&dir);
    assert!(values(&ask("drive_instance", "event", "", "", &store)).is_empty());
    assert!(values(&ask("drive_instance", "event", "pu", "", &store)).is_empty());
    // An instance that does not exist is the same answer.
    assert!(values(&ask("drive_instance", "event", "", "inst-nope", &store)).is_empty());
}

#[test]
fn an_instance_with_nothing_enabled_offers_nothing() {
    let dir = scratch("settled");
    let mut store = seeded(&dir);
    // Drive one instance into its terminal state, where nothing is enabled.
    store
        .send_event(
            "inst-0",
            "push",
            Value::Obj(BTreeMap::new()),
            "push-1",
            None,
        )
        .unwrap();
    let offered = values(&ask("drive_instance", "event", "", "inst-0", &store));
    assert!(
        offered.is_empty(),
        "held has no outgoing transitions: {offered:?}"
    );
}

#[test]
fn a_free_text_goal_has_no_candidates() {
    let dir = scratch("goal");
    let store = seeded(&dir);
    assert!(values(&ask("author_machine", "goal", "", "", &store)).is_empty());
    assert!(values(&ask("author_machine", "instance_id", "", "", &store)).is_empty());
}
