use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
#[cfg(test)]
use fsm_core::record::RecordKind;

use crate::args::{Args, CmdSpec, Ctx, read_input_from};
use crate::render::{emit_error, emit_success};
use crate::store::{
    ErrorObj, Store, apply_context_overrides, coerce_ctx_override, context_not_object,
};

fn open(ctx: &Ctx) -> Result<Store, ErrorObj> {
    Store::open(&ctx.data_dir)
}

fn open_read_only(ctx: &Ctx) -> Result<Store, ErrorObj> {
    Store::open_read_only(&ctx.data_dir)
}

fn fail(ctx: &Ctx, e: ErrorObj, rid: &str) -> u8 {
    emit_error(ctx, &e.request_id(rid))
}

fn new_inst(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(mref) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance new <machine>"));
    };
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let rid = match args.flags.get("request-id").cloned() {
        Some(r) => r,
        None => match store.allocate_request_id() {
            Ok(r) => r,
            Err(e) => return emit_error(ctx, &e),
        },
    };
    let iid = format!("inst-{rid}");
    let mut overrides = BTreeMap::new();
    if let Some(pairs) = args.flags.get("context") {
        let m = match store.resolve_machine(mref) {
            Ok(m) => m,
            Err(e) => return fail(ctx, e, &rid),
        };
        for part in pairs.split(',') {
            if let Some((k, val)) = part.split_once('=') {
                let Some(decl) = m.compiled.spec.context.iter().find(|c| c.name == k) else {
                    return fail(ctx, ErrorObj::new("req/field_unknown", k), &rid);
                };
                match coerce_ctx_override(&decl.ty, k, val) {
                    Ok(v) => {
                        overrides.insert(k.to_string(), v);
                    }
                    Err(e) => return fail(ctx, e, &rid),
                }
            }
        }
    }
    if let Some(j) = args.flags.get("context-json") {
        let text = match read_input_from(j, ctx.stdin.as_deref()) {
            Ok(s) => s,
            Err(e) => return fail(ctx, e, &rid),
        };
        match parse(text.as_bytes(), &JsonLimits::DEFAULT) {
            Ok(Value::Obj(o)) => {
                let m = match store.resolve_machine(mref) {
                    Ok(m) => m,
                    Err(e) => return fail(ctx, e, &rid),
                };
                match apply_context_overrides(&m.compiled.spec, &o) {
                    Ok(part) => overrides.extend(part),
                    Err(e) => return fail(ctx, e, &rid),
                }
            }
            Ok(Value::Arr(_)) => return fail(ctx, context_not_object("array"), &rid),
            Ok(_) => return fail(ctx, context_not_object("not-object"), &rid),
            Err(e) => return fail(ctx, ErrorObj::new("def/shape", e.message), &rid),
        }
    }
    match store.create_instance_ctx(mref, &iid, &rid, None, &overrides, &[]) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => fail(ctx, e, &rid),
    }
}

fn send(ctx: &mut Ctx, args: &Args) -> u8 {
    if args.positionals.len() < 2 {
        return emit_error(ctx, &ErrorObj::new("args", "instance send <id> <event>"));
    }
    let iid = &args.positionals[0];
    let ev = &args.positionals[1];
    let mut payload = if let Some(p) = args.flags.get("payload") {
        match read_input_from(p, ctx.stdin.as_deref()).and_then(|s| {
            parse(s.as_bytes(), &JsonLimits::DEFAULT)
                .map_err(|e| ErrorObj::new("def/shape", e.message))
        }) {
            Ok(v) => v,
            Err(e) => return emit_error(ctx, &e),
        }
    } else {
        Value::Obj(BTreeMap::new())
    };
    let expect = match args.flags.get("expect-seq") {
        None => None,
        Some(s) => match s.parse::<u64>() {
            Ok(n) => Some(n),
            Err(_) => {
                return emit_error(
                    ctx,
                    &ErrorObj::new("args", "expect-seq must be a u64")
                        .hint("pass an integer sequence"),
                );
            }
        },
    };
    let stamp = args.flags.get("stamp").map(String::as_str);
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let rid = match args.flags.get("request-id").cloned() {
        Some(r) => r,
        None => match store.allocate_request_id() {
            Ok(r) => r,
            Err(e) => return emit_error(ctx, &e),
        },
    };
    let stamps: Vec<&str> = stamp.into_iter().collect();
    match store.send_event_stamp(iid, ev, &mut payload, &rid, expect, &stamps) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => fail(ctx, e, &rid),
    }
}

fn poll_deadline(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(instance_id) = args.positionals.first() else {
        return emit_error(
            ctx,
            &ErrorObj::new("req/args_invalid", "instance poll requires an instance id")
                .hint("use: instance poll <id>"),
        );
    };
    let expect_seq = match args.flags.get("expect-seq") {
        None => None,
        Some(raw) => match raw.parse::<u64>() {
            Ok(sequence) => Some(sequence),
            Err(_) => {
                return emit_error(
                    ctx,
                    &ErrorObj::new("req/args_invalid", "expect-seq must be a u64")
                        .hint("pass an integer sequence"),
                );
            }
        },
    };
    let mut store = match open(ctx) {
        Ok(store) => store,
        Err(error) => return emit_error(ctx, &error),
    };
    let request_id = match args.flags.get("request-id").cloned() {
        Some(request_id) => request_id,
        None => match store.allocate_request_id() {
            Ok(request_id) => request_id,
            Err(error) => return emit_error(ctx, &error),
        },
    };
    match store.poll_instance_deadline(instance_id, &request_id, expect_seq) {
        Ok(value) => {
            emit_success(ctx, &value);
            0
        }
        Err(error) => fail(ctx, error, &request_id),
    }
}

/// `fsm instance invoke <parent> <slot>` — create the child of a pending
/// slot. The executor normally does this; this is the path for a session
/// with none running.
fn invoke(ctx: &mut Ctx, args: &Args) -> u8 {
    composition(
        ctx,
        args,
        "instance invoke <id> <slot>",
        |store, one, two, rid| store.invoke_child(one, two, rid),
    )
}

/// `fsm instance return <parent> <slot>` — hand a settled child's result to
/// its parent, where it arrives as `$done.invoke.<slot>`.
fn invocation_return(ctx: &mut Ctx, args: &Args) -> u8 {
    composition(
        ctx,
        args,
        "instance return <id> <slot>",
        |store, one, two, rid| store.invocation_return(one, two, rid),
    )
}

/// `fsm instance signal <sender> <signal-id>` — deliver one pending signal.
fn signal(ctx: &mut Ctx, args: &Args) -> u8 {
    composition(
        ctx,
        args,
        "instance signal <id> <signal-id>",
        |store, one, two, rid| store.signal_deliver(one, two, rid),
    )
}

/// The three composition commands differ only in which mutator they call.
fn composition(
    ctx: &mut Ctx,
    args: &Args,
    usage: &str,
    call: impl FnOnce(&mut crate::store::Store, &str, &str, &str) -> Result<Value, ErrorObj>,
) -> u8 {
    if args.positionals.len() < 2 {
        return emit_error(ctx, &ErrorObj::new("args", usage));
    }
    let mut store = match open(ctx) {
        Ok(store) => store,
        Err(error) => return emit_error(ctx, &error),
    };
    let rid = match args.flags.get("request-id").cloned() {
        Some(rid) => rid,
        None => match store.allocate_request_id() {
            Ok(rid) => rid,
            Err(error) => return emit_error(ctx, &error),
        },
    };
    match call(&mut store, &args.positionals[0], &args.positionals[1], &rid) {
        Ok(value) => {
            emit_success(ctx, &value);
            0
        }
        Err(error) => fail(ctx, error, &rid),
    }
}

fn ack(ctx: &mut Ctx, args: &Args) -> u8 {
    if args.positionals.len() < 2 {
        return emit_error(ctx, &ErrorObj::new("args", "instance ack <id> <effect>"));
    }
    let Some(outcome) = args.flags.get("outcome").map(String::as_str) else {
        return emit_error(
            ctx,
            &ErrorObj::new("args", "outcome is required").hint("pass --outcome ok|failed"),
        );
    };
    if outcome != "ok" && outcome != "failed" {
        return emit_error(ctx, &ErrorObj::new("args", "outcome must be ok or failed"));
    }
    let result = match args.flags.get("result") {
        None => None,
        Some(src) => match read_input_from(src, ctx.stdin.as_deref()).and_then(|s| {
            parse(s.as_bytes(), &JsonLimits::DEFAULT)
                .map_err(|e| ErrorObj::new("def/shape", e.message))
        }) {
            Ok(v) => Some(v),
            Err(e) => return emit_error(ctx, &e),
        },
    };
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let rid = match args.flags.get("request-id").cloned() {
        Some(r) => r,
        None => match store.allocate_request_id() {
            Ok(r) => r,
            Err(e) => return emit_error(ctx, &e),
        },
    };
    match store.ack_effect_outcome(
        &args.positionals[0],
        &args.positionals[1],
        &rid,
        outcome,
        result,
    ) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => fail(ctx, e, &rid),
    }
}

fn cancel(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance cancel <id>"));
    };
    let Some(reason) = args.flags.get("reason").cloned() else {
        return emit_error(
            ctx,
            &ErrorObj::new("req/field_missing", "reason is required").hint("pass --reason"),
        );
    };
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let rid = match args.flags.get("request-id").cloned() {
        Some(r) => r,
        None => match store.allocate_request_id() {
            Ok(r) => r,
            Err(e) => return emit_error(ctx, &e),
        },
    };
    match store.cancel_instance_reason(id, &rid, &reason) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => fail(ctx, e, &rid),
    }
}

fn annotate(ctx: &mut Ctx, args: &Args) -> u8 {
    if args.positionals.len() < 2 {
        return emit_error(ctx, &ErrorObj::new("args", "instance annotate <id> <text>"));
    }
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let rid = match args.flags.get("request-id").cloned() {
        Some(r) => r,
        None => match store.allocate_request_id() {
            Ok(r) => r,
            Err(e) => return emit_error(ctx, &e),
        },
    };
    match store.annotate(&args.positionals[0], &rid, &args.positionals[1]) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => fail(ctx, e, &rid),
    }
}

fn show(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance show <id>"));
    };
    let store = match open_read_only(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.instance_view(id, None, None) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn ls(ctx: &mut Ctx, args: &Args) -> u8 {
    let store = match open_read_only(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let machine_f = args.flags.get("machine").cloned();
    let state_f = args.flags.get("state").cloned();
    let status_f = args.flags.get("status").cloned();
    let mut rows = Vec::new();
    for (id, inst) in &store.state.instances {
        if let Some(st) = &status_f {
            if st != "all" && inst.status.as_str() != st {
                continue;
            }
        }
        if let Some(sf) = &state_f {
            let matches = match &inst.configuration {
                fsm_core::machine::ActiveConfiguration::Sequential { leaf } => leaf == sf,
                fsm_core::machine::ActiveConfiguration::Parallel { leaves } => {
                    leaves.values().any(|leaf| leaf == sf)
                }
            };
            if !matches {
                continue;
            }
        }
        if let Some(mf) = &machine_f {
            let mid = store.state.instance_machines.get(id);
            if mid.map(|m| !m.contains(mf.as_str())).unwrap_or(true) {
                continue;
            }
        }
        let mut row = BTreeMap::new();
        row.insert("instance_id".into(), Value::Str(id.clone()));
        row.insert(
            "configuration".into(),
            fsm_core::hashes::configuration_value(&inst.configuration),
        );
        match &inst.configuration {
            fsm_core::machine::ActiveConfiguration::Sequential { leaf } => {
                row.insert("state".into(), Value::Str(leaf.clone()));
            }
            fsm_core::machine::ActiveConfiguration::Parallel { leaves } => {
                row.insert(
                    "regions".into(),
                    Value::Obj(
                        leaves
                            .iter()
                            .map(|(region, leaf)| (region.clone(), Value::Str(leaf.clone())))
                            .collect(),
                    ),
                );
            }
        }
        row.insert("status".into(), Value::Str(inst.status.as_str().into()));
        rows.push(Value::Obj(row));
    }
    emit_success(
        ctx,
        &Value::Obj(BTreeMap::from([("instances".into(), Value::Arr(rows))])),
    );
    0
}

fn history(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance history <id>"));
    };
    let store = match open_read_only(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let from = args
        .flags
        .get("from-seq")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let limit = args
        .flags
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(50);
    let want_trace = args.switches.contains("trace");
    let include_rejected = !args.switches.contains("hide-rejected");
    match store.history_page(id, from, limit, want_trace, include_rejected) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn explain(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "explain <instance>"));
    };
    let Some(seq) = args.flags.get("seq").and_then(|s| s.parse::<u64>().ok()) else {
        return emit_error(ctx, &ErrorObj::new("args", "explain --seq N"));
    };
    let store = match open_read_only(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.explain_seq(id, seq) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

pub static SPECS: &[CmdSpec] = &[
    CmdSpec {
        path: &["instance", "new"],
        positionals: &["machine"],
        flags: &["request-id", "context", "context-json"],
        switches: &[],
        help: "Create instance",
        run: new_inst,
    },
    CmdSpec {
        path: &["instance", "send"],
        positionals: &["instance", "event"],
        flags: &["payload", "request-id", "expect-seq", "stamp"],
        switches: &[],
        help: "Send event",
        run: send,
    },
    CmdSpec {
        path: &["instance", "poll"],
        positionals: &["instance"],
        flags: &["request-id", "expect-seq"],
        switches: &[],
        help: "Poll one due deadline",
        run: poll_deadline,
    },
    CmdSpec {
        path: &["instance", "ack"],
        positionals: &["instance", "effect"],
        flags: &["outcome", "result", "request-id"],
        switches: &[],
        help: "Ack effect",
        run: ack,
    },
    CmdSpec {
        path: &["instance", "invoke"],
        positionals: &["instance", "slot"],
        flags: &["request-id"],
        switches: &[],
        help: "Invoke a slot's child",
        run: invoke,
    },
    CmdSpec {
        path: &["instance", "return"],
        positionals: &["instance", "slot"],
        flags: &["request-id"],
        switches: &[],
        help: "Return a settled child's result",
        run: invocation_return,
    },
    CmdSpec {
        path: &["instance", "signal"],
        positionals: &["instance", "signal-id"],
        flags: &["request-id"],
        switches: &[],
        help: "Deliver one pending signal",
        run: signal,
    },
    CmdSpec {
        path: &["instance", "cancel"],
        positionals: &["instance"],
        flags: &["reason", "request-id"],
        switches: &[],
        help: "Cancel instance",
        run: cancel,
    },
    CmdSpec {
        path: &["instance", "annotate"],
        positionals: &["instance", "text"],
        flags: &["request-id"],
        switches: &[],
        help: "Annotate",
        run: annotate,
    },
    CmdSpec {
        path: &["instance", "show"],
        positionals: &["instance"],
        flags: &[],
        switches: &[],
        help: "Show instance",
        run: show,
    },
    CmdSpec {
        path: &["instance", "ls"],
        positionals: &[],
        flags: &["machine", "state", "status"],
        switches: &[],
        help: "List instances",
        run: ls,
    },
    CmdSpec {
        path: &["instance", "history"],
        positionals: &["instance"],
        flags: &["from-seq", "limit"],
        switches: &["trace", "hide-rejected"],
        help: "Instance history",
        run: history,
    },
    CmdSpec {
        path: &["explain"],
        positionals: &["instance"],
        flags: &["seq"],
        switches: &[],
        help: "Explain a step",
        run: explain,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Ctx;
    use crate::cli::machine;
    use crate::clock;
    use fsm_core::json::Value;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmp() -> std::path::PathBuf {
        let n = N.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("fsm-ic-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn case() -> String {
        format!(
            "{}/../fsm-core/tests/fixtures/machines/case_review.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn setup() -> (std::path::PathBuf, String) {
        clock::reset_injected();
        clock::force_ms(5_000);
        crate::args::reset_request_ids();
        let dir = tmp();
        let mut c = Ctx::new(dir.clone(), true, false);
        assert_eq!(
            (machine::SPECS[0].run)(
                &mut c,
                &Args {
                    positionals: vec![case()],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            0
        );
        let mut store = Store::open(&dir).unwrap();
        let r = store
            .create_instance("case_review", "i1", "c1", None)
            .unwrap();
        let iid = r
            .get("instance_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        (dir, iid)
    }

    #[test]
    fn new_prints_leaf_and_request() {
        let dir = tmp();
        let mut c = Ctx::new(dir.clone(), true, false);
        (machine::SPECS[0].run)(
            &mut c,
            &Args {
                positionals: vec![case()],
                flags: BTreeMap::new(),
                switches: Default::default(),
            },
        );
        let code = new_inst(
            &mut c,
            &Args {
                positionals: vec!["case_review".into()],
                flags: BTreeMap::new(),
                switches: Default::default(),
            },
        );
        assert_eq!(code, 0);
        let store = Store::open(&dir).unwrap();
        let inst = store.state.instances.values().next().unwrap();
        assert_eq!(inst.configuration.sequential_leaf(), Some("intake"));
    }

    #[test]
    fn over_precision_context() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let spec = r#"{"format":"fsm.machine/1","name":"decm","context":[{"name":"amt","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"start"},{"name":"a","terminal":true}],"initial":"start","transitions":[{"from":"start","on":"go","to":"a"}]}"#;
        let v = parse(spec.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        store.define_machine(v, false, false).unwrap();
        drop(store);
        let mut c = Ctx::new(dir.clone(), true, false);
        let code = new_inst(
            &mut c,
            &Args {
                positionals: vec!["decm".into()],
                flags: BTreeMap::from([("context".into(), "amt=1.505".into())]),
                switches: Default::default(),
            },
        );
        assert_eq!(code, 1);
        assert!(Store::open(&dir).unwrap().state.instances.is_empty());
    }

    #[test]
    fn send_applied_rejected_idempotent_seq() {
        let (dir, iid) = setup();
        let mut c = Ctx::new(dir.clone(), true, false);
        let seq = Store::open(&dir).unwrap().journal.last_seq;
        assert_eq!(
            send(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "docs_ok".into()],
                    flags: BTreeMap::from([
                        ("request-id".into(), "R1".into()),
                        ("expect-seq".into(), seq.to_string())
                    ]),
                    switches: Default::default()
                }
            ),
            0
        );
        assert_eq!(
            send(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "scored".into()],
                    flags: BTreeMap::from([("request-id".into(), "bad".into())]),
                    switches: Default::default()
                }
            ),
            1
        );
        let n = Store::open(&dir).unwrap().journal.last_seq;
        assert_eq!(
            send(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "docs_ok".into()],
                    flags: BTreeMap::from([("request-id".into(), "R1".into())]),
                    switches: Default::default()
                }
            ),
            0
        );
        assert_eq!(Store::open(&dir).unwrap().journal.last_seq, n);
        assert_eq!(
            send(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "note_added".into()],
                    flags: BTreeMap::from([
                        ("request-id".into(), "stale".into()),
                        ("expect-seq".into(), "0".into()),
                        ("payload".into(), r#"{"text":"hi"}"#.into())
                    ]),
                    switches: Default::default()
                }
            ),
            1
        );
    }

    #[test]
    fn stamp_ack_cancel_annotate_show_ls_history() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let spec = r#"{"format":"fsm.machine/1","name":"tsm","context":[],"events":[{"name":"tick","fields":[{"name":"at","ty":"timestamp"}]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"tick","to":"b"}]}"#;
        let v = parse(spec.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        store.define_machine(v, false, false).unwrap();
        store.create_instance("tsm", "t1", "c", None).unwrap();
        drop(store);
        clock::force_ms(42_000);
        let mut c = Ctx::new(dir.clone(), true, false);
        assert_eq!(
            send(
                &mut c,
                &Args {
                    positionals: vec!["t1".into(), "tick".into()],
                    flags: BTreeMap::from([
                        ("request-id".into(), "st".into()),
                        ("stamp".into(), "at".into()),
                        ("payload".into(), "{}".into())
                    ]),
                    switches: Default::default()
                }
            ),
            0
        );
        let store = Store::open(&dir).unwrap();
        let rec = store
            .records
            .iter()
            .rev()
            .find(|r| r.kind == RecordKind::EventApplied)
            .unwrap();
        let at = rec
            .body
            .get("payload")
            .and_then(|p| p.get("at"))
            .and_then(Value::as_str)
            .unwrap();
        assert!(!at.is_empty(), "{at}");
        drop(store);

        let (dir2, iid) = setup();
        let mut c = Ctx::new(dir2.clone(), true, false);
        send(
            &mut c,
            &Args {
                positionals: vec![iid.clone(), "docs_ok".into()],
                flags: BTreeMap::from([("request-id".into(), "d1".into())]),
                switches: Default::default(),
            },
        );
        let pending = Store::open(&dir2)
            .unwrap()
            .state
            .instances
            .get(&iid)
            .unwrap()
            .pending
            .clone();
        if let Some(eid) = pending.first() {
            assert_eq!(
                ack(
                    &mut c,
                    &Args {
                        positionals: vec![iid.clone(), eid.clone()],
                        flags: BTreeMap::from([("outcome".into(), "ok".into())]),
                        switches: Default::default()
                    }
                ),
                0
            );
        }
        assert_eq!(
            ack(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "nope".into()],
                    flags: BTreeMap::from([("outcome".into(), "ok".into())]),
                    switches: Default::default()
                }
            ),
            1
        );
        assert_eq!(
            annotate(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "hello-note".into()],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            0
        );
        assert_eq!(
            show(
                &mut c,
                &Args {
                    positionals: vec![iid.clone()],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            0
        );
        assert_eq!(
            ls(
                &mut c,
                &Args {
                    positionals: vec![],
                    flags: BTreeMap::from([("status".into(), "running".into())]),
                    switches: Default::default()
                }
            ),
            0
        );
        assert_eq!(
            history(
                &mut c,
                &Args {
                    positionals: vec![iid.clone()],
                    flags: BTreeMap::from([
                        ("from-seq".into(), "0".into()),
                        ("limit".into(), "10".into())
                    ]),
                    switches: BTreeSet::from(["trace"])
                }
            ),
            0
        );
        assert_eq!(
            cancel(
                &mut c,
                &Args {
                    positionals: vec![iid.clone()],
                    flags: BTreeMap::from([("reason".into(), "done".into())]),
                    switches: Default::default()
                }
            ),
            0
        );
        assert_eq!(
            send(
                &mut c,
                &Args {
                    positionals: vec![iid.clone(), "docs_ok".into()],
                    flags: BTreeMap::from([("request-id".into(), "after".into())]),
                    switches: Default::default()
                }
            ),
            1
        );
        let recs = Store::open(&dir2).unwrap().records.clone();
        if let Some(r) = recs.iter().find(|r| r.kind == RecordKind::EventApplied) {
            assert_eq!(
                explain(
                    &mut c,
                    &Args {
                        positionals: vec![iid],
                        flags: BTreeMap::from([("seq".into(), r.seq.to_string())]),
                        switches: Default::default()
                    }
                ),
                0
            );
        }
    }
}
