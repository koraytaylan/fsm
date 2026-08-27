use std::collections::BTreeMap;

use crate::mcp::progress::ProgressReporter;
use fsm_core::json::Value;
use fsm_core::simulate::{OnReject, simulate};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store, coerce_ctx_override};

use crate::mcp::tools::dispatch::str_arg;

pub(in crate::mcp::tools) fn run_simulate(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    run_simulate_with(store, clock, args, &ProgressReporter::discarding())
}

pub(in crate::mcp::tools) fn run_simulate_with(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, ErrorObj> {
    let has_spec = args.get("spec").is_some();
    let has_machine = str_arg(args, "machine").is_some();
    if has_spec && has_machine {
        return Err(ErrorObj::new(
            "req/args_invalid",
            "provide machine or spec, not both",
        ));
    }
    let compiled = if let Some(specv) = args.get("spec") {
        compile_accepted(specv).map_err(ErrorObj::from_findings)?
    } else if let Some(r) = str_arg(args, "machine") {
        store.resolve_machine(r)?.compiled.clone()
    } else {
        return Err(ErrorObj::new(
            "req/args_invalid",
            "machine or spec required",
        ));
    };
    let tree = Tree::for_machine(&compiled.spec);
    let mut events = Vec::new();
    if let Some(arr) = args.get("events").and_then(Value::as_arr) {
        for item in arr {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let payload = item
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            events.push((name, payload));
        }
    }
    let on = match str_arg(args, "on_reject") {
        Some("continue") => OnReject::Continue,
        _ => OnReject::Stop,
    };
    let mut overrides = BTreeMap::new();
    if let Some(ctx) = args.get("context") {
        let Value::Obj(obj) = ctx else {
            return Err(ErrorObj::new(
                "req/args_invalid",
                "context must be an object",
            ));
        };
        for (k, val) in obj {
            let decl = compiled
                .spec
                .context
                .iter()
                .find(|c| c.name == *k)
                .ok_or_else(|| ErrorObj::new("req/field_unknown", k.clone()))?;
            let raw = match val {
                Value::Str(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Num(_) => {
                    return Err(ErrorObj::new("req/number_token", k.clone()));
                }
                _ => return Err(ErrorObj::new("req/field_type", k.clone())),
            };
            let v = coerce_ctx_override(&decl.ty, k, &raw)?;
            overrides.insert(k.clone(), v);
        }
    }
    let created = fsm_core::step::create(&compiled, &tree, &overrides, 0)
        .map_err(|r| ErrorObj::from_rejection(&r))?;
    let mut initial_ctx = BTreeMap::new();
    for (k, v) in &created.ctx_after {
        initial_ctx.insert(k.clone(), fsm_core::replay::ctx_val_json(v));
    }
    let report = simulate(&compiled, &tree, &overrides, &events, on)
        .map_err(|rejection| ErrorObj::from_rejection(&rejection))?;
    let mut from_configuration = created.configuration_after.clone();
    let mut steps = Vec::new();
    // The size is known up front, so every report carries its denominator.
    let total = report.steps.len() as u64;
    for (index, st) in report.steps.iter().enumerate() {
        let done = index as u64 + 1;
        progress.report(clock.now_ms(), done, Some(total), None, done == total);
        let mut m = BTreeMap::new();
        m.insert("index".into(), Value::Num(st.index.to_string()));
        m.insert("event".into(), Value::Str(st.event.clone()));
        m.insert(
            "from_configuration".into(),
            fsm_core::hashes::configuration_value(&from_configuration),
        );
        m.insert(
            "to_configuration".into(),
            fsm_core::hashes::configuration_value(&st.configuration_after),
        );
        if let (
            fsm_core::machine::ActiveConfiguration::Sequential { leaf: from_leaf },
            fsm_core::machine::ActiveConfiguration::Sequential { leaf: to_leaf },
        ) = (&from_configuration, &st.configuration_after)
        {
            m.insert("from_leaf".into(), Value::Str(from_leaf.clone()));
            m.insert("to_leaf".into(), Value::Str(to_leaf.clone()));
        }
        let args = events
            .get(st.index)
            .map(|(_, p)| p.clone())
            .unwrap_or(Value::Obj(BTreeMap::new()));
        m.insert("args".into(), args);
        m.insert(
            "applied".into(),
            Value::Bool(matches!(st.outcome, fsm_core::step::Outcome::Applied(_))),
        );
        let mut ctx = BTreeMap::new();
        for (k, v) in &st.ctx_after {
            ctx.insert(k.clone(), fsm_core::replay::ctx_val_json(v));
        }
        m.insert("context".into(), Value::Obj(ctx));
        m.insert(
            "effects".into(),
            Value::Arr(
                st.effects
                    .iter()
                    .map(|ef| {
                        let mut em = BTreeMap::new();
                        em.insert("effect".into(), Value::Str(ef.name.clone()));
                        em.insert("k".into(), Value::Num(ef.k.to_string()));
                        let mut args = BTreeMap::new();
                        for (k, v) in &ef.args {
                            args.insert(k.clone(), fsm_core::replay::ctx_val_json(v));
                        }
                        em.insert("args".into(), Value::Obj(args));
                        Value::Obj(em)
                    })
                    .collect(),
            ),
        );
        match &st.outcome {
            fsm_core::step::Outcome::Applied(a) => {
                if let Some(region) = &a.region {
                    m.insert("region".into(), Value::Str(region.clone()));
                }
                m.insert("trace".into(), a.trace.to_value());
                // The record shape, so a model sees one vocabulary for a
                // cascade across simulate, instance_history, and explain.
                if let Some(microsteps) = fsm_core::record::microsteps_value(&a.trace.microsteps) {
                    m.insert("microsteps".into(), microsteps);
                }
            }
            fsm_core::step::Outcome::Rejected(r) => {
                let mut err = BTreeMap::new();
                err.insert("code".into(), Value::Str(r.code.into()));
                err.insert("message".into(), Value::Str(r.message.clone()));
                err.insert("hint".into(), Value::Str(r.hint.clone()));
                m.insert("error".into(), Value::Obj(err));
                m.insert("trace".into(), r.trace.to_value());
            }
            fsm_core::step::Outcome::Ignored => {
                m.insert("ignored".into(), Value::Bool(true));
                m.insert("trace".into(), Value::Obj(BTreeMap::new()));
            }
        }
        if !m.contains_key("trace") {
            m.insert("trace".into(), Value::Obj(BTreeMap::new()));
        }
        from_configuration = st.configuration_after.clone();
        steps.push(Value::Obj(m));
    }
    let mut final_ctx = initial_ctx.clone();
    if let Some(last) = report.steps.last() {
        final_ctx.clear();
        for (k, v) in &last.ctx_after {
            final_ctx.insert(k.clone(), fsm_core::replay::ctx_val_json(v));
        }
    }
    let mut initial = BTreeMap::from([
        (
            "configuration".into(),
            fsm_core::hashes::configuration_value(&created.configuration_after),
        ),
        ("context".into(), Value::Obj(initial_ctx)),
    ]);
    if let Some(leaf) = created.configuration_after.sequential_leaf() {
        initial.insert("state".into(), Value::Str(tree.dotted_path(leaf)));
    }
    let mut final_state = BTreeMap::from([
        (
            "configuration".into(),
            fsm_core::hashes::configuration_value(&report.final_configuration),
        ),
        ("terminal".into(), Value::Bool(report.terminal)),
        ("context".into(), Value::Obj(final_ctx)),
    ]);
    if let Some(leaf) = report.final_configuration.sequential_leaf() {
        final_state.insert("state".into(), Value::Str(tree.dotted_path(leaf)));
    }
    let mut out = BTreeMap::from([
        ("initial".into(), Value::Obj(initial)),
        ("steps".into(), Value::Arr(steps)),
        ("final".into(), Value::Obj(final_state)),
    ]);
    if let Some(n) = report.stopped_at {
        out.insert("stopped_at".into(), Value::Num(n.to_string()));
    }
    Ok(Value::Obj(out))
}
