//! Acceptance: the full library loop, driven from outside `fsm-core`.
//!
//! This is the library counterpart to the CLI and MCP host checks in
//! `docs/RELEASE.md`. It fails if the in-process embedding path regresses —
//! including the persistence round-trip an embedder with its own store depends
//! on.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_embed_acceptance::{advance, coverage, digest, from_row, lint, load, start, to_row};

const SPEC: &[u8] = include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

fn payload(pairs: &[(&str, &str)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::Str((*v).to_string())))
            .collect(),
    )
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
    let st = start(&m, &BTreeMap::new()).expect("create");
    assert_eq!(st.leaf, "intake");
    assert_eq!(st.status, Status::Running);

    let a = advance(&m, &st, "docs_ok", &payload(&[]))
        .expect("step")
        .expect("docs_ok is handled at intake");
    assert_eq!(a.next.leaf, "docs_review");
    assert_eq!(a.entered, vec!["in_review", "docs_review"]);
    assert_eq!(a.exited, vec!["intake"]);
    assert_eq!(
        a.next.ctx.get("visits"),
        Some(&Val::Int(1)),
        "the entry block must have run"
    );
    assert_eq!(a.effects.len(), 1, "entry emitted one effect");

    // Stepping is pure: the input state is untouched.
    assert_eq!(st.leaf, "intake");

    let a2 = advance(&m, &a.next, "docs_ok", &payload(&[]))
        .expect("step")
        .expect("handled");
    let a3 = advance(&m, &a2.next, "scored", &payload(&[("score", "800")]))
        .expect("step")
        .expect("handled");
    assert_eq!(a3.next.leaf, "approved");
    assert_eq!(
        a3.next.status,
        Status::Completed,
        "terminal state completes"
    );
}

/// The round-trip an embedder with its own persistence lives on: encode with
/// the engine's own encoding, store it, read it back, and keep stepping. The
/// state digest must be identical across the boundary — that is the check that
/// catches silent drift.
#[test]
fn instance_state_survives_the_embedder_s_own_store() {
    let m = load(SPEC).expect("spec compiles");
    let st = start(&m, &BTreeMap::new()).expect("create");
    let a = advance(&m, &st, "docs_ok", &payload(&[]))
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
    assert_eq!(restored.leaf, a.next.leaf);
    assert_eq!(restored.history, a.next.history);
    assert_eq!(restored.status, a.next.status);
    assert_eq!(
        digest(mid, "i1", 7, &restored),
        before,
        "a persisted-then-restored instance must hash identically"
    );

    // ...and it is still steppable.
    let next = advance(&m, &restored, "docs_ok", &payload(&[]))
        .expect("step")
        .expect("handled");
    assert_eq!(next.next.leaf, "risk_review");
}

/// Overrides go in as typed values and come back out unchanged.
#[test]
fn context_overrides_round_trip() {
    let m = load(SPEC).expect("spec compiles");
    let overrides = BTreeMap::from([("visits".to_string(), Val::Int(41))]);
    let st = start(&m, &overrides).expect("create with overrides");
    // The entry block on the creation chain increments it.
    assert_eq!(st.ctx.get("visits"), Some(&Val::Int(41)));

    let restored = from_row(&m, &to_row(&st)).expect("row decodes");
    assert_eq!(restored.ctx.get("visits"), Some(&Val::Int(41)));
}

/// Errors are values with codes, not panics — an embedder can route on them.
#[test]
fn rejections_are_typed_not_panics() {
    let m = load(SPEC).expect("spec compiles");
    let st = start(&m, &BTreeMap::new()).expect("create");
    match advance(&m, &st, "resume", &payload(&[])) {
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
