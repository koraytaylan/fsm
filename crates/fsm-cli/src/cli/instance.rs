use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;

use crate::args::{Args, CmdSpec, Ctx, default_request_id, read_input_from};
use crate::render::{emit_error, emit_success};
use crate::store::{ErrorObj, Store, coerce_ctx_override};

fn open(ctx: &Ctx) -> Result<Store, ErrorObj> {
    Store::open(&ctx.data_dir)
}

fn new_inst(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(mref) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance new <machine>"));
    };
    let rid = args
        .flags
        .get("request-id")
        .cloned()
        .unwrap_or_else(default_request_id);
    let iid = format!("inst-{rid}");
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let mut overrides = BTreeMap::new();
    if let Some(pairs) = args.flags.get("context") {
        let m = match store.resolve_machine(mref) {
            Ok(m) => m,
            Err(e) => return emit_error(ctx, &e),
        };
        for part in pairs.split(',') {
            if let Some((k, val)) = part.split_once('=') {
                let Some(decl) = m.compiled.spec.context.iter().find(|c| c.name == k) else {
                    return emit_error(ctx, &ErrorObj::new("req/field_unknown", k));
                };
                match coerce_ctx_override(&decl.ty, k, val) {
                    Ok(v) => {
                        overrides.insert(k.to_string(), v);
                    }
                    Err(e) => return emit_error(ctx, &e),
                }
            }
        }
    }
    if let Some(j) = args.flags.get("context-json") {
        let text = match read_input_from(j, ctx.stdin.as_deref()) {
            Ok(s) => s,
            Err(e) => return emit_error(ctx, &e),
        };
        match parse(text.as_bytes(), &JsonLimits::DEFAULT) {
            Ok(Value::Obj(o)) => {
                let m = match store.resolve_machine(mref) {
                    Ok(m) => m,
                    Err(e) => return emit_error(ctx, &e),
                };
                for (k, val) in o {
                    let raw = match val {
                        Value::Str(s) => s,
                        Value::Num(_n) => {
                            return emit_error(ctx, &ErrorObj::new("req/number_token", k));
                        }
                        Value::Bool(b) => b.to_string(),
                        _ => {
                            return emit_error(ctx, &ErrorObj::new("req/field_type", k));
                        }
                    };
                    let Some(decl) = m.compiled.spec.context.iter().find(|c| c.name == k) else {
                        return emit_error(ctx, &ErrorObj::new("req/field_unknown", &k));
                    };
                    match coerce_ctx_override(&decl.ty, &k, &raw) {
                        Ok(v) => {
                            overrides.insert(k, v);
                        }
                        Err(e) => return emit_error(ctx, &e),
                    }
                }
            }
            _ => return emit_error(ctx, &ErrorObj::new("req/args_invalid", "context-json")),
        }
    }
    match store.create_instance_ctx(mref, &iid, &rid, None, &overrides) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
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
    let rid = args
        .flags
        .get("request-id")
        .cloned()
        .unwrap_or_else(default_request_id);
    let expect = args
        .flags
        .get("expect-seq")
        .and_then(|s| s.parse::<u64>().ok());
    let stamp = args.flags.get("stamp").map(String::as_str);
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.send_event_stamp(iid, ev, &mut payload, &rid, expect, stamp) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn ack(ctx: &mut Ctx, args: &Args) -> u8 {
    if args.positionals.len() < 2 {
        return emit_error(ctx, &ErrorObj::new("args", "instance ack <id> <effect>"));
    }
    let rid = args
        .flags
        .get("request-id")
        .cloned()
        .unwrap_or_else(default_request_id);
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.ack_effect(&args.positionals[0], &args.positionals[1], &rid) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn cancel(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance cancel <id>"));
    };
    let reason = args.flags.get("reason").cloned().unwrap_or_default();
    let rid = default_request_id();
    let mut store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.cancel_instance_reason(id, &rid, &reason) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
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
    match store.annotate(
        &args.positionals[0],
        &default_request_id(),
        &args.positionals[1],
    ) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn show(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "instance show <id>"));
    };
    let store = match open(ctx) {
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
    let store = match open(ctx) {
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
            if &inst.leaf != sf {
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
        row.insert("state".into(), Value::Str(inst.leaf.clone()));
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
    let store = match open(ctx) {
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
    let mut entries = Vec::new();
    for rec in store
        .records
        .iter()
        .filter(|r| {
            r.body.get("instance_id").and_then(Value::as_str) == Some(id.as_str()) && r.seq >= from
        })
        .take(limit)
    {
        let mut e = BTreeMap::new();
        e.insert("seq".into(), Value::Num(rec.seq.to_string()));
        e.insert("kind".into(), Value::Str(format!("{:?}", rec.kind)));
        if let Some(ev) = rec.body.get("event") {
            e.insert("event".into(), ev.clone());
        }
        if let Some(p) = rec.body.get("payload") {
            e.insert("payload".into(), p.clone());
        }
        if let Some(n) = rec.body.get("note") {
            e.insert("note".into(), n.clone());
        }
        if let Some(r) = rec.body.get("reason") {
            e.insert("reason".into(), r.clone());
        }
        if want_trace {
            e.insert("trace".into(), Value::Str("recomputed".into()));
        }
        entries.push(Value::Obj(e));
    }
    emit_success(
        ctx,
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(id.clone())),
            ("entries".into(), Value::Arr(entries)),
        ])),
    );
    0
}

fn explain(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(id) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "explain <instance>"));
    };
    let Some(seq) = args.flags.get("seq").and_then(|s| s.parse::<u64>().ok()) else {
        return emit_error(ctx, &ErrorObj::new("args", "explain --seq N"));
    };
    let store = match open(ctx) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let rec = store.records.iter().find(|r| r.seq == seq);
    let Some(rec) = rec else {
        return emit_error(ctx, &ErrorObj::new("req/field_missing", "seq"));
    };
    let _ = id;
    let mut m = BTreeMap::new();
    m.insert("seq".into(), Value::Num(seq.to_string()));
    m.insert("kind".into(), Value::Str(format!("{:?}", rec.kind)));
    if rec.kind == RecordKind::EventApplied {
        m.insert("trace".into(), Value::Str("recomputed".into()));
    }
    emit_success(ctx, &Value::Obj(m));
    0
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
        path: &["instance", "ack"],
        positionals: &["instance", "effect"],
        flags: &["outcome", "request-id"],
        switches: &[],
        help: "Ack effect",
        run: ack,
    },
    CmdSpec {
        path: &["instance", "cancel"],
        positionals: &["instance"],
        flags: &["reason"],
        switches: &[],
        help: "Cancel instance",
        run: cancel,
    },
    CmdSpec {
        path: &["instance", "annotate"],
        positionals: &["instance", "text"],
        flags: &[],
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
        switches: &["trace"],
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
        assert_eq!(inst.leaf, "intake");
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
                        flags: BTreeMap::new(),
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
                    flags: BTreeMap::new(),
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
        let recs = Store::open(&dir2).unwrap().records;
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
