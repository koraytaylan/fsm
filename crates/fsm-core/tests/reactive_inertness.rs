//! The reactive plan's compatibility contract, written as a suite: a
//! definition that uses none of the reactive features — no eventless
//! transition, no `raise`, no `final` state — produces, byte for byte, the
//! machine ids, traces, state hashes, tick counts, and genesis limits the
//! build before the plan produced. Every golden here was written by that
//! build (`FSM_REGEN_FIXTURES=1` in a checkout of the pre-plan commit), so a
//! failing assertion means the plan has broken its own contract, and the
//! golden is not what gets edited.
//!
//! Plan 0009 task 4603.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{STATE_FORMAT, machine_id, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MAX_EVAL_TICKS;
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::record::limits_value;
use fsm_core::spec::{MachineSpec, TySpec, compile, load_machine_json};
use fsm_core::step::{Applied, DeadlineOutcome, Outcome, create, poll_deadline, step};
use fsm_core::tree::Tree;

const GOLDEN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/inertness/drives.json"
);
const IDENTITY: &str = include_str!("fixtures/hashes/identity.jsonl");

/// Every committed machine the goldens rely on: the shipped examples and
/// the fixture the step, record, select, shadowing, and hash goldens drive.
const MACHINES: &[(&str, &[u8])] = &[
    (
        "case_review",
        include_bytes!("fixtures/machines/case_review.json"),
    ),
    (
        "expense_approval",
        include_bytes!("../../../examples/expense_approval.json"),
    ),
    (
        "invoice_matching",
        include_bytes!("../../../examples/invoice_matching.json"),
    ),
    (
        "order_lifecycle",
        include_bytes!("../../../examples/order_lifecycle.json"),
    ),
    (
        "parallel_review_deadline",
        include_bytes!("../../../examples/parallel_review_deadline.json"),
    ),
];

fn document(bytes: &[u8]) -> Value {
    parse(bytes, &JsonLimits::DEFAULT).unwrap()
}

fn canonical(value: &Value) -> String {
    String::from_utf8(canon_bytes(value)).unwrap()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// Reactive syntax, found by walking the document rather than the parsed
/// spec, so the same check reads the same on the pre-plan build.
fn reactive_syntax(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Obj(object) => {
            if object.contains_key("raise") {
                found.push(format!("{path}/raise"));
            }
            if object.get("final") == Some(&Value::Bool(true)) {
                found.push(format!("{path}/final"));
            }
            let mut segments = path.rsplit('/');
            let is_transition = segments
                .next()
                .is_some_and(|last| !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()))
                && segments.next() == Some("transitions");
            if is_transition && !object.contains_key("on") {
                found.push(format!("{path} (eventless)"));
            }
            for (key, child) in object {
                reactive_syntax(child, &format!("{path}/{key}"), found);
            }
        }
        Value::Arr(items) => {
            for (index, child) in items.iter().enumerate() {
                reactive_syntax(child, &format!("{path}/{index}"), found);
            }
        }
        _ => {}
    }
}

fn machine(bytes: &[u8]) -> (CompiledMachine, Tree) {
    let spec = load_machine_json(bytes).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn instance(applied: &Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after.clone(),
        ctx: applied.ctx_after.clone(),
        history: applied.history_after.clone(),
        deadlines: applied.deadlines_after.clone(),
        pending: Vec::new(),
    }
}

/// A payload of typed zero values, so every declared event gets one
/// deterministic attempt whatever it needs.
fn zero_payload(spec: &MachineSpec, event: &str) -> Value {
    let fields = spec
        .events
        .iter()
        .find(|declared| declared.name == event)
        .map(|declared| declared.fields.as_slice())
        .unwrap_or(&[]);
    Value::Obj(
        fields
            .iter()
            .map(|field| {
                let literal = match &field.ty {
                    TySpec::Int | TySpec::Ts | TySpec::Dur => "0".to_string(),
                    TySpec::Dec { scale } => {
                        if *scale == 0 {
                            "0".to_string()
                        } else {
                            format!("0.{}", "0".repeat(*scale as usize))
                        }
                    }
                    TySpec::Str | TySpec::Enum { .. } => String::new(),
                    TySpec::Bool => "false".to_string(),
                };
                (field.name.clone(), Value::Str(literal))
            })
            .collect(),
    )
}

/// Create, then attempt every declared event in document order with a zero
/// payload, then poll far enough ahead that any schedule is due, recording
/// each outcome with its state hash, its trace, and the ticks it cost.
fn drive(name: &str, bytes: &[u8]) -> Value {
    let id = machine_id(&document(bytes));
    let (m, t) = machine(bytes);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut state = instance(&created);
    let mut seq = 1;
    let mut out = BTreeMap::new();
    out.insert("name".to_string(), s(name));
    out.insert("machine_id".to_string(), s(&id));
    out.insert(
        "create".to_string(),
        Value::Obj(BTreeMap::from([
            (
                "state_hash".to_string(),
                s(&state_hash(&id, "inertness", seq, &state)),
            ),
            (
                "trace".to_string(),
                s(&canonical(&created.trace.to_value())),
            ),
        ])),
    );
    let events: Vec<String> = m.spec.events.iter().map(|e| e.name.clone()).collect();
    let mut steps = Vec::new();
    for (i, event) in events.iter().enumerate() {
        let now = 1_000 * (i as i64 + 1);
        let mut budget = Budget::new(MAX_EVAL_TICKS);
        let outcome = step(
            &m,
            &t,
            &state,
            event,
            &zero_payload(&m.spec, event),
            now,
            &mut budget,
        );
        let mut entry = BTreeMap::from([
            ("event".to_string(), s(event)),
            (
                "ticks".to_string(),
                Value::Num((MAX_EVAL_TICKS - budget.remaining()).to_string()),
            ),
        ]);
        match outcome {
            Outcome::Applied(applied) => {
                seq += 1;
                state = instance(&applied);
                entry.insert("outcome".to_string(), s("applied"));
                entry.insert(
                    "state_hash".to_string(),
                    s(&state_hash(&id, "inertness", seq, &state)),
                );
                entry.insert(
                    "trace".to_string(),
                    s(&canonical(&applied.trace.to_value())),
                );
            }
            Outcome::Rejected(rejection) => {
                entry.insert(
                    "outcome".to_string(),
                    s(&format!("rejected {}", rejection.code)),
                );
                entry.insert(
                    "trace".to_string(),
                    s(&canonical(&rejection.trace.to_value())),
                );
            }
            Outcome::Ignored => {
                entry.insert("outcome".to_string(), s("ignored"));
            }
        }
        steps.push(Value::Obj(entry));
    }
    out.insert("steps".to_string(), Value::Arr(steps));
    let mut budget = Budget::new(MAX_EVAL_TICKS);
    let mut poll = BTreeMap::new();
    match poll_deadline(&m, &t, &state, 10_000_000, &mut budget) {
        DeadlineOutcome::Applied(applied) => {
            seq += 1;
            state = instance(&applied.transition);
            poll.insert("outcome".to_string(), s("applied"));
            poll.insert("deadline".to_string(), s(&applied.deadline.name));
            poll.insert(
                "state_hash".to_string(),
                s(&state_hash(&id, "inertness", seq, &state)),
            );
            poll.insert(
                "trace".to_string(),
                s(&canonical(&applied.transition.trace.to_value())),
            );
        }
        DeadlineOutcome::NotDue { next } => {
            poll.insert("outcome".to_string(), s("not_due"));
            if let Some(next) = next {
                poll.insert("next".to_string(), s(&next.name));
            }
        }
        DeadlineOutcome::Rejected(rejected) => {
            poll.insert(
                "outcome".to_string(),
                s(&format!("rejected {}", rejected.rejection.code)),
            );
        }
    }
    poll.insert(
        "ticks".to_string(),
        Value::Num((MAX_EVAL_TICKS - budget.remaining()).to_string()),
    );
    out.insert("poll".to_string(), Value::Obj(poll));
    Value::Obj(out)
}

fn drives() -> Value {
    Value::Obj(BTreeMap::from([
        (
            "machines".to_string(),
            Value::Arr(
                MACHINES
                    .iter()
                    .map(|(name, bytes)| drive(name, bytes))
                    .collect(),
            ),
        ),
        ("genesis_limits".to_string(), limits_value()),
    ]))
}

#[test]
fn every_committed_machine_is_non_reactive_and_keeps_its_identity() {
    let committed: BTreeSet<String> = IDENTITY
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            document(line.as_bytes())
                .get("id")
                .and_then(Value::as_str)
                .unwrap()
                .to_string()
        })
        .collect();
    for (name, bytes) in MACHINES {
        let doc = document(bytes);
        let mut found = Vec::new();
        reactive_syntax(&doc, "", &mut found);
        assert!(found.is_empty(), "{name} uses reactive syntax at {found:?}");
        let id = machine_id(&doc);
        assert!(
            committed.contains(&id),
            "{name}'s id {id} is not the one committed in identity.jsonl"
        );
    }
}

#[test]
fn drives_match_the_pre_plan_build_byte_for_byte() {
    let bytes = canon_bytes(&drives());
    if std::env::var_os("FSM_REGEN_FIXTURES").is_some() {
        std::fs::create_dir_all(Path::new(GOLDEN).parent().unwrap()).unwrap();
        std::fs::write(GOLDEN, &bytes).unwrap();
    }
    let committed = std::fs::read(GOLDEN).expect("the pre-plan drives golden is committed");
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        !text.contains("microsteps") && !text.contains("internal_unhandled"),
        "a non-reactive drive carries a reaction key"
    );
    assert!(
        bytes == committed,
        "a non-reactive machine's hashes, traces, or tick counts moved from the pre-plan build"
    );
}

#[test]
fn genesis_limits_carry_no_reactive_ceiling() {
    let limits = limits_value();
    let keys: Vec<&String> = limits.as_obj().unwrap().keys().collect();
    assert!(
        keys.iter()
            .all(|key| !key.contains("microstep") && !key.contains("raise")),
        "{keys:?}"
    );
    let committed = document(&std::fs::read(GOLDEN).unwrap());
    assert_eq!(committed.get("genesis_limits"), Some(&limits));
}

#[test]
fn the_state_format_is_unchanged_and_no_newer_one_exists() {
    assert_eq!(STATE_FORMAT, "fsm.state/2");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut versions = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let mut rest = text.as_str();
                while let Some(at) = rest.find("fsm.state/") {
                    let tail = &rest[at + "fsm.state/".len()..];
                    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
                    if !digits.is_empty() {
                        versions.insert(digits);
                    }
                    rest = tail;
                }
            }
        }
    }
    assert_eq!(
        versions,
        BTreeSet::from(["1".to_string(), "2".to_string()]),
        "a state format other than the legacy 1 and the current 2 appears in the workspace"
    );
}

#[test]
fn a_reactive_machine_does_produce_the_key() {
    // Negative control: the suite would notice if the key were never emitted.
    let src = br#"{"format":"fsm.machine/1","name":"reactive","states":[{"name":"a"},{"name":"b"},{"name":"c"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","to":"c"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MAX_EVAL_TICKS);
    let Outcome::Applied(applied) = step(
        &m,
        &t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) else {
        panic!("go applies");
    };
    assert!(canonical(&applied.trace.to_value()).contains("\"microsteps\""));
}
