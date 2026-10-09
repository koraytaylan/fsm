//! Original durable native claim fixtures and admission refusals.

use super::*;

pub(super) fn claim_binding(fixture: &Fixture, domain: &NativeDomain) -> (Value, String) {
    claim_binding_paths(&fixture.directory, &fixture.store, domain)
}

pub(super) fn claim_binding_paths(
    directory: &Path,
    store_path: &Path,
    domain: &NativeDomain,
) -> (Value, String) {
    let mut store = Store::open(store_path).unwrap();
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
    let approved = super::super::super::catalogue::read(directory).unwrap();
    let handler = &approved.handlers["notify"];
    let before = store.records.clone();
    let state = store.state.clone();
    let mut wrong = handler.clone();
    wrong.effect = "different_effect".into();
    let mut pipeline = fsm_execute::run::Pipeline;
    assert_eq!(
        pipeline
            .claim_native_handler(
                &mut store,
                &mut FixedClock::new(100, 1),
                &effect,
                &wrong,
                domain,
                "wrong-handler-claim"
            )
            .unwrap_err()
            .code,
        "exec/config"
    );
    assert_eq!(store.records, before);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert!(!store.state.dedup.contains_key("wrong-handler-claim"));
    let mut invalid = handler.clone();
    invalid.retry.attempts = 0;
    assert_eq!(
        pipeline
            .claim_native_handler(
                &mut store,
                &mut FixedClock::new(100, 1),
                &effect,
                &invalid,
                domain,
                "invalid-handler-claim"
            )
            .unwrap_err()
            .code,
        "exec/config"
    );
    assert_eq!(store.records, before);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert!(!store.state.dedup.contains_key("invalid-handler-claim"));
    for field in ["argv", "mcp_arguments", "on_ok", "on_failed"] {
        let mut excessive = handler.clone();
        if field == "argv" {
            excessive
                .argv
                .push("x".repeat(JsonLimits::DEFAULT.max_bytes + 1));
        } else {
            let mut payload = Value::Null;
            for _ in 0..=JsonLimits::DEFAULT.max_depth {
                payload = Value::Arr(vec![payload]);
            }
            if field == "mcp_arguments" {
                excessive.kind = fsm_execute::config::HandlerKind::Mcp {
                    tool: "notify".into(),
                    arguments: payload,
                };
            } else {
                let advance = Some(fsm_execute::config::Advance {
                    event: "done".into(),
                    payload,
                    stamps: Vec::new(),
                });
                if field == "on_ok" {
                    excessive.on_ok = advance;
                } else {
                    excessive.on_failed = advance;
                }
            }
        }
        let error = pipeline
            .claim_native_handler(
                &mut store,
                &mut FixedClock::new(100, 1),
                &effect,
                &excessive,
                domain,
                "oversized-handler-claim",
            )
            .unwrap_err();
        assert_eq!(error.code, "exec/config");
        assert_eq!(error.message, "native handler input exceeds JSON bounds");
        assert_eq!(store.records, before);
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
        assert!(!store.state.dedup.contains_key("oversized-handler-claim"));
    }
    let claim = pipeline
        .claim_native_handler(
            &mut store,
            &mut FixedClock::new(100, 1),
            &effect,
            handler,
            domain,
            "claim",
        )
        .unwrap();
    assert_eq!(
        claim,
        *store
            .state
            .execution
            .claim_for("instance", &effect)
            .unwrap()
    );
    let records = store.records.clone();
    let hash = store.current_execution_claim_hash(&claim).unwrap();
    let duplicate = pipeline
        .claim_native_handler(
            &mut store,
            &mut FixedClock::new(100, 1),
            &effect,
            handler,
            domain,
            "claim",
        )
        .unwrap();
    assert_eq!(duplicate, claim);
    assert_eq!(store.records, records);
    assert_eq!(store.current_execution_claim_hash(&claim).unwrap(), hash);
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
            Value::Str(
                store
                    .current_execution_claim_hash(
                        store
                            .state
                            .execution
                            .claim_for("instance", &effect)
                            .unwrap(),
                    )
                    .unwrap(),
            ),
        ),
    ]);
    drop(store);
    (binding, effect)
}
