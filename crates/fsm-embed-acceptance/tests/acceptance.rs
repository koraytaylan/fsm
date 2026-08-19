//! Acceptance: the full library loop, driven from outside `fsm-core`.
//!
//! This is the library counterpart to the CLI and MCP host checks in
//! `docs/RELEASE.md`. It fails if the in-process embedding path regresses —
//! including the persistence round-trip an embedder with its own store depends
//! on.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MAX_EVAL_TICKS;
use fsm_core::machine::{ActiveConfiguration, Status};
use fsm_core::simulate::{OnReject, simulate};
use fsm_embed_acceptance::{
    DeadlinePoll, EVAL_BUDGET, advance, coverage, digest, from_row, lint, load, poll_deadline,
    start, to_row,
};

const SPEC: &[u8] = include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

const PARALLEL_DEADLINE_SPEC: &[u8] = br#"{
    "format":"fsm.machine/1",
    "name":"parallel_deadline",
    "regions":[
        {
            "name":"work",
            "states":[
                {"name":"waiting"},
                {"name":"timed_out","terminal":true}
            ],
            "initial":"waiting"
        },
        {
            "name":"audit",
            "states":[
                {"name":"checking"},
                {"name":"approved","terminal":true}
            ],
            "initial":"checking"
        }
    ],
    "context":[],
    "events":[{"name":"approve","fields":[]}],
    "transitions":[{"from":"checking","on":"approve","to":"approved"}],
    "deadlines":[
        {"name":"work_timeout","from":"waiting","after":"dur(5, ms)","to":"timed_out"}
    ]
}"#;

const SHALLOW_HISTORY_SPEC: &[u8] = br#"{
    "format":"fsm.machine/1",
    "name":"shallow_history_rows",
    "states":[
        {"name":"idle"},
        {"name":"work","initial":"phase","states":[
            {"name":"work_history","history":"shallow"},
            {"name":"phase","initial":"one","states":[{"name":"one"}]},
            {"name":"other"}
        ]}
    ],
    "initial":"idle",
    "context":[],
    "events":[],
    "transitions":[]
}"#;

const PARALLEL_HISTORY_SPEC: &[u8] = br#"{
    "format":"fsm.machine/1",
    "name":"parallel_history_rows",
    "regions":[
        {"name":"work","initial":"idle","states":[
            {"name":"idle"},
            {"name":"job","initial":"doing","states":[
                {"name":"job_history","history":"deep"},
                {"name":"doing"}
            ]}
        ]},
        {"name":"audit","initial":"checking","states":[{"name":"checking"}]}
    ],
    "context":[],
    "events":[],
    "transitions":[]
}"#;

#[test]
fn downstream_standard_budget_matches_the_compiler_ceiling() {
    assert_eq!(EVAL_BUDGET, MAX_EVAL_TICKS);
    assert_eq!(MAX_EVAL_TICKS, 4096);
}

fn payload(pairs: &[(&str, &str)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::Str((*v).to_string())))
            .collect(),
    )
}

fn object_mut(value: &mut Value) -> &mut BTreeMap<String, Value> {
    match value {
        Value::Obj(object) => object,
        other => panic!("expected object, got {other:?}"),
    }
}

fn persistence_error(machine: &fsm_embed_acceptance::Machine, row: &Value) -> String {
    match from_row(machine, row) {
        Err(fsm_embed_acceptance::EmbedError::Persistence(message)) => message,
        Err(other) => panic!("expected persistence error, got {other:?}"),
        Ok(_) => panic!("malformed row unexpectedly decoded"),
    }
}

/// parse → compile → analyze → create → step → completeness_matrix, in process,
/// with no `Store` anywhere. This is the path stages 1–2 of an embedding use.
#[test]
fn library_loop_end_to_end() {
    let m = load(SPEC).expect("spec compiles");
    assert_eq!(m.compiled.spec.name, "case_review");

    // Static analysis is available before anything runs.
    let findings = lint(&m);
    assert!(
        findings
            .iter()
            .all(|f| f.severity != fsm_core::spec::Severity::Error),
        "reference machine must be clean: {findings:?}"
    );

    // The completeness matrix is the embedder's coverage check.
    let matrix = coverage(&m);
    assert!(
        !matrix.is_empty(),
        "matrix must describe (leaf, event) pairs"
    );
    assert!(
        matrix
            .keys()
            .any(|(leaf, ev)| leaf == "intake" && ev == "docs_ok"),
        "matrix must cover the initial leaf"
    );

    // Create, then drive.
    let st = start(&m, &BTreeMap::new(), 0).expect("create");
    assert_eq!(st.configuration.sequential_leaf(), Some("intake"));
    assert_eq!(st.status, Status::Running);

    let a = advance(&m, &st, "docs_ok", &payload(&[]), 1)
        .expect("step")
        .expect("docs_ok is handled at intake");
    assert_eq!(a.next.configuration.sequential_leaf(), Some("docs_review"));
    assert_eq!(a.entered, vec!["in_review", "docs_review"]);
    assert_eq!(a.exited, vec!["intake"]);
    assert_eq!(
        a.next.ctx.get("visits"),
        Some(&Val::Int(1)),
        "the entry block must have run"
    );
    assert_eq!(a.effects.len(), 1, "entry emitted one effect");

    // Stepping is pure: the input state is untouched.
    assert_eq!(st.configuration.sequential_leaf(), Some("intake"));

    let a2 = advance(&m, &a.next, "docs_ok", &payload(&[]), 2)
        .expect("step")
        .expect("handled");
    let a3 = advance(&m, &a2.next, "scored", &payload(&[("score", "800")]), 3)
        .expect("step")
        .expect("handled");
    assert_eq!(a3.next.configuration.sequential_leaf(), Some("approved"));
    assert_eq!(
        a3.next.status,
        Status::Completed,
        "terminal state completes"
    );

    // The public what-if helper exposes creation failure through Result; an
    // accepted report therefore always contains a real active configuration.
    let simulation = simulate(
        &m.compiled,
        &m.tree,
        &BTreeMap::new(),
        &[("docs_ok".into(), payload(&[]))],
        OnReject::Stop,
    )
    .expect("simulation creation succeeds");
    assert_eq!(
        simulation.final_configuration.sequential_leaf(),
        Some("docs_review")
    );
}

/// The round-trip an embedder with its own persistence lives on: encode with
/// the engine's own encoding, store it, read it back, and keep stepping. The
/// state digest must be identical across the boundary — that is the check that
/// catches silent drift.
#[test]
fn instance_state_survives_the_embedder_s_own_store() {
    let m = load(SPEC).expect("spec compiles");
    let st = start(&m, &BTreeMap::new(), 0).expect("create");
    let a = advance(&m, &st, "docs_ok", &payload(&[]), 1)
        .expect("step")
        .expect("handled");

    let mid = &m.compiled.machine_id;
    let before = digest(mid, "i1", 7, &a.next);

    // Through bytes, exactly as a real store would: serialize, persist, parse.
    let row = to_row(&a.next);
    let bytes = fsm_core::canon::canon_bytes(&row);
    let reread = parse(&bytes, &JsonLimits::DEFAULT).expect("row is valid JSON");
    let restored = from_row(&m, &reread).expect("row decodes");

    assert_eq!(restored.ctx, a.next.ctx, "context must survive verbatim");
    assert_eq!(restored.configuration, a.next.configuration);
    assert_eq!(restored.history, a.next.history);
    assert_eq!(restored.deadlines, a.next.deadlines);
    assert_eq!(restored.status, a.next.status);
    assert_eq!(
        digest(mid, "i1", 7, &restored),
        before,
        "a persisted-then-restored instance must hash identically"
    );

    // ...and it is still steppable.
    let next = advance(&m, &restored, "docs_ok", &payload(&[]), 2)
        .expect("step")
        .expect("handled");
    assert_eq!(
        next.next.configuration.sequential_leaf(),
        Some("risk_review")
    );
}

/// Overrides go in as typed values and come back out unchanged.
#[test]
fn context_overrides_round_trip() {
    let m = load(SPEC).expect("spec compiles");
    let overrides = BTreeMap::from([("visits".to_string(), Val::Int(41))]);
    let st = start(&m, &overrides, 0).expect("create with overrides");
    // The entry block on the creation chain increments it.
    assert_eq!(st.ctx.get("visits"), Some(&Val::Int(41)));

    let restored = from_row(&m, &to_row(&st)).expect("row decodes");
    assert_eq!(restored.ctx.get("visits"), Some(&Val::Int(41)));
}

/// Errors are values with codes, not panics — an embedder can route on them.
#[test]
fn rejections_are_typed_not_panics() {
    let m = load(SPEC).expect("spec compiles");
    let st = start(&m, &BTreeMap::new(), 0).expect("create");
    match advance(&m, &st, "resume", &payload(&[]), 1) {
        Err(fsm_embed_acceptance::EmbedError::Rejected(code)) => {
            assert_eq!(code, "run/unhandled");
        }
        other => panic!(
            "expected a typed rejection, got {other:?}",
            other = match other {
                Ok(Some(_)) => "applied".to_string(),
                Ok(None) => "ignored".to_string(),
                Err(e) => format!("{e:?}"),
            }
        ),
    }
}

/// Parallel configurations and deadline schedules survive the same external
/// persistence boundary. Polling is explicit and pure: an early poll leaves
/// the row untouched; a due poll advances one region; an ordinary event can
/// then finish the other region.
#[test]
fn parallel_deadline_state_can_reload_poll_and_continue() {
    let m = load(PARALLEL_DEADLINE_SPEC).expect("parallel deadline spec compiles");
    let created = start(&m, &BTreeMap::new(), 100).expect("create");
    assert!(matches!(
        &created.configuration,
        ActiveConfiguration::Parallel { leaves }
            if leaves.get("work").map(String::as_str) == Some("waiting")
                && leaves.get("audit").map(String::as_str) == Some("checking")
    ));
    assert_eq!(created.deadlines.get("work_timeout"), Some(&105));

    let bytes = fsm_core::canon::canon_bytes(&to_row(&created));
    let row = parse(&bytes, &JsonLimits::DEFAULT).expect("row parses");
    let restored = from_row(&m, &row).expect("parallel row decodes");
    assert_eq!(restored.configuration, created.configuration);
    assert_eq!(restored.deadlines, created.deadlines);
    assert_eq!(
        digest(&m.compiled.machine_id, "p1", 1, &restored),
        digest(&m.compiled.machine_id, "p1", 1, &created)
    );

    match poll_deadline(&m, &restored, 104).expect("early poll") {
        DeadlinePoll::NotDue { next: Some(next) } => {
            assert_eq!(next.name, "work_timeout");
            assert_eq!(next.due_ms, 105);
        }
        _ => panic!("the schedule is not due yet"),
    }
    assert_eq!(restored.deadlines.get("work_timeout"), Some(&105));

    let timed_out = match poll_deadline(&m, &restored, 105).expect("due poll") {
        DeadlinePoll::Applied { deadline, advance } => {
            assert_eq!(deadline.name, "work_timeout");
            advance
        }
        DeadlinePoll::NotDue { .. } => panic!("the schedule must be due"),
    };
    assert_eq!(timed_out.next.status, Status::Running);
    assert!(timed_out.next.deadlines.is_empty());
    assert!(matches!(
        &timed_out.next.configuration,
        ActiveConfiguration::Parallel { leaves }
            if leaves.get("work").map(String::as_str) == Some("timed_out")
                && leaves.get("audit").map(String::as_str) == Some("checking")
    ));

    let persisted = from_row(&m, &to_row(&timed_out.next)).expect("timed row decodes");
    let completed = advance(&m, &persisted, "approve", &payload(&[]), 106)
        .expect("event step")
        .expect("audit region handles approve");
    assert_eq!(completed.next.status, Status::Completed);
    assert!(matches!(
        completed.next.configuration,
        ActiveConfiguration::Parallel { ref leaves }
            if leaves.get("work").map(String::as_str) == Some("timed_out")
                && leaves.get("audit").map(String::as_str) == Some("approved")
    ));
}

#[test]
fn persisted_rows_reject_unknown_top_level_and_configuration_fields() {
    let machine = load(SPEC).expect("spec compiles");
    let state = start(&machine, &BTreeMap::new(), 0).expect("create");

    let mut extra_top_level = to_row(&state);
    object_mut(&mut extra_top_level).insert("future".into(), Value::Bool(true));
    assert!(
        persistence_error(&machine, &extra_top_level).contains("row contains unknown field future")
    );

    let mut extra_sequential_field = to_row(&state);
    let configuration = object_mut(&mut extra_sequential_field)
        .get_mut("configuration")
        .expect("configuration field");
    object_mut(configuration).insert("leaves".into(), Value::Obj(BTreeMap::new()));
    assert!(
        persistence_error(&machine, &extra_sequential_field)
            .contains("configuration contains unknown field leaves")
    );

    let parallel = load(PARALLEL_DEADLINE_SPEC).expect("parallel spec compiles");
    let parallel_state = start(&parallel, &BTreeMap::new(), 0).expect("parallel create");
    let mut extra_parallel_field = to_row(&parallel_state);
    let configuration = object_mut(&mut extra_parallel_field)
        .get_mut("configuration")
        .expect("configuration field");
    object_mut(configuration).insert("leaf".into(), Value::Str("waiting".into()));
    assert!(
        persistence_error(&parallel, &extra_parallel_field)
            .contains("configuration contains unknown field leaf")
    );
}

#[test]
fn persisted_rows_require_exact_active_deadlines_and_coherent_lifecycle() {
    let machine = load(PARALLEL_DEADLINE_SPEC).expect("parallel deadline spec compiles");
    let created = start(&machine, &BTreeMap::new(), 100).expect("create");

    let mut missing = to_row(&created);
    object_mut(
        object_mut(&mut missing)
            .get_mut("deadlines")
            .expect("deadline field"),
    )
    .clear();
    assert!(persistence_error(&machine, &missing).contains("missing: [\"work_timeout\"]"));

    let mut inactive = to_row(&created);
    let configuration = object_mut(&mut inactive)
        .get_mut("configuration")
        .expect("configuration field");
    let leaves = object_mut(configuration)
        .get_mut("leaves")
        .expect("leaves field");
    object_mut(leaves).insert("work".into(), Value::Str("timed_out".into()));
    assert!(persistence_error(&machine, &inactive).contains("unexpected: [\"work_timeout\"]"));

    let mut completed_nonterminal = to_row(&created);
    object_mut(&mut completed_nonterminal).insert("status".into(), Value::Str("completed".into()));
    assert!(persistence_error(&machine, &completed_nonterminal).contains("completed status"));

    let mut cancelled_with_schedule = to_row(&created);
    object_mut(&mut cancelled_with_schedule)
        .insert("status".into(), Value::Str("cancelled".into()));
    assert!(
        persistence_error(&machine, &cancelled_with_schedule)
            .contains("unexpected: [\"work_timeout\"]")
    );

    let mut running_terminal = to_row(&created);
    let row = object_mut(&mut running_terminal);
    object_mut(row.get_mut("deadlines").expect("deadlines field")).clear();
    let configuration = object_mut(row.get_mut("configuration").expect("configuration field"));
    let leaves = object_mut(configuration.get_mut("leaves").expect("leaves field"));
    leaves.insert("work".into(), Value::Str("timed_out".into()));
    leaves.insert("audit".into(), Value::Str("approved".into()));
    assert!(persistence_error(&machine, &running_terminal).contains("running status"));
}

#[test]
fn persisted_history_must_match_owner_mode_and_ancestry() {
    let deep = load(SPEC).expect("deep-history spec compiles");
    let deep_state = start(&deep, &BTreeMap::new(), 0).expect("create");

    let mut owner_without_history = to_row(&deep_state);
    object_mut(
        object_mut(&mut owner_without_history)
            .get_mut("history")
            .expect("history field"),
    )
    .insert("intake".into(), Value::Str("intake".into()));
    assert!(persistence_error(&deep, &owner_without_history).contains("is not a compound state"));

    let mut deep_compound = to_row(&deep_state);
    object_mut(
        object_mut(&mut deep_compound)
            .get_mut("history")
            .expect("history field"),
    )
    .insert("in_review".into(), Value::Str("in_review".into()));
    assert!(persistence_error(&deep, &deep_compound).contains("not a descendant"));

    let mut deep_outside = to_row(&deep_state);
    object_mut(
        object_mut(&mut deep_outside)
            .get_mut("history")
            .expect("history field"),
    )
    .insert("in_review".into(), Value::Str("intake".into()));
    assert!(persistence_error(&deep, &deep_outside).contains("not a descendant"));

    let shallow = load(SHALLOW_HISTORY_SPEC).expect("shallow-history spec compiles");
    let shallow_state = start(&shallow, &BTreeMap::new(), 0).expect("create");
    let mut nested_shallow = to_row(&shallow_state);
    object_mut(
        object_mut(&mut nested_shallow)
            .get_mut("history")
            .expect("history field"),
    )
    .insert("work".into(), Value::Str("one".into()));
    assert!(persistence_error(&shallow, &nested_shallow).contains("must name a direct child"));

    let parallel = load(PARALLEL_HISTORY_SPEC).expect("parallel-history spec compiles");
    let parallel_state = start(&parallel, &BTreeMap::new(), 0).expect("create");
    let mut cross_region = to_row(&parallel_state);
    object_mut(
        object_mut(&mut cross_region)
            .get_mut("history")
            .expect("history field"),
    )
    .insert("job".into(), Value::Str("checking".into()));
    assert!(persistence_error(&parallel, &cross_region).contains("not a descendant"));

    let mut valid_shallow = to_row(&shallow_state);
    object_mut(
        object_mut(&mut valid_shallow)
            .get_mut("history")
            .expect("history field"),
    )
    .insert("work".into(), Value::Str("phase".into()));
    from_row(&shallow, &valid_shallow).expect("direct compound is valid shallow history");
}

#[test]
fn a_bad_definition_reports_findings_with_paths() {
    let Err(err) = load(br#"{"format":"fsm.machine/1","name":"x","states":[{"name":"a"}],"initial":"nope","context":[],"events":[],"transitions":[]}"#)
    else {
        panic!("initial names an unknown state, so this must not compile");
    };
    match err {
        fsm_embed_acceptance::EmbedError::Definition(findings) => {
            assert!(!findings.is_empty());
            assert!(
                findings.iter().all(|f| !f.hint.is_empty()),
                "every finding carries a hint the embedder can surface"
            );
        }
        other => panic!("expected definition findings, got {other:?}"),
    }
}
