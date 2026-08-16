use std::collections::BTreeMap;

use fsm_core::analyze::{analyze_all, completeness_matrix};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::tree::Tree;

use crate::args::{Args, CmdSpec, Ctx, read_input_from};
use crate::render::emit_error;
use crate::render::emit_success;
use crate::store::{ErrorObj, Store};

fn add(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(p) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "machine add <spec>"));
    };
    let text = match read_input_from(p, ctx.stdin.as_deref()) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let def = match parse(text.as_bytes(), &JsonLimits::DEFAULT) {
        Ok(v) => v,
        Err(e) => return emit_error(ctx, &ErrorObj::new("def/shape", e.message)),
    };
    let mut store = match Store::open(&ctx.data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let strict = args.flags.get("if-exists").map(String::as_str) == Some("error");
    match store.define_machine(def, false, strict) {
        Ok(o) => {
            let mut m = BTreeMap::new();
            m.insert("machine_id".into(), Value::Str(o.machine_id));
            m.insert("created".into(), Value::Bool(o.created));
            m.insert("name".into(), Value::Str(o.name));
            m.insert("dry_run".into(), Value::Bool(false));
            m.insert(
                "warnings".into(),
                Value::Arr(
                    o.warnings
                        .into_iter()
                        .map(|f| Value::Str(f.code.into()))
                        .collect(),
                ),
            );
            emit_success(ctx, &Value::Obj(m));
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn ls(ctx: &mut Ctx, args: &Args) -> u8 {
    let store = match Store::open(&ctx.data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let filter = args.flags.get("name-contains").cloned();
    let mut rows = Vec::new();
    for (id, m) in &store.state.machines {
        let name = &m.compiled.spec.name;
        if let Some(f) = &filter {
            if !name.contains(f.as_str()) {
                continue;
            }
        }
        let insts = store
            .state
            .instance_machines
            .values()
            .filter(|mid| *mid == id)
            .count();
        let mut row = BTreeMap::new();
        row.insert("machine_id".into(), Value::Str(id.clone()));
        row.insert("name".into(), Value::Str(name.clone()));
        row.insert(
            "states".into(),
            Value::Num(m.compiled.spec.states.len().to_string()),
        );
        row.insert(
            "events".into(),
            Value::Num(m.compiled.spec.events.len().to_string()),
        );
        row.insert("instances".into(), Value::Num(insts.to_string()));
        rows.push(Value::Obj(row));
    }
    emit_success(
        ctx,
        &Value::Obj(BTreeMap::from([("machines".into(), Value::Arr(rows))])),
    );
    0
}

fn show(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(r) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "machine show <id>"));
    };
    let store = match Store::open(&ctx.data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.resolve_machine(r) {
        Ok(m) => {
            let canon = String::from_utf8(canon_bytes(&m.def)).unwrap_or_default();
            let mut summary = BTreeMap::new();
            summary.insert(
                "initial".into(),
                Value::Str(m.compiled.spec.initial.clone()),
            );
            let terminals: Vec<Value> = m
                .compiled
                .spec
                .states
                .iter()
                .filter(|s| s.terminal)
                .map(|s| Value::Str(s.name.clone()))
                .collect();
            summary.insert("terminal_states".into(), Value::Arr(terminals));
            let mut obj = BTreeMap::new();
            obj.insert("spec".into(), Value::Str(canon));
            obj.insert("summary".into(), Value::Obj(summary));
            obj.insert(
                "machine_id".into(),
                Value::Str(fsm_core::hashes::machine_id(&m.def)),
            );
            emit_success(ctx, &Value::Obj(obj));
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

fn analyze(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(r) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "machine analyze <id>"));
    };
    let store = match Store::open(&ctx.data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    match store.resolve_machine(r) {
        Ok(m) => {
            let t = Tree::build(&m.compiled.spec.states);
            let findings = analyze_all(&m.compiled, &t);
            let has_err = findings
                .iter()
                .any(|f| f.severity == fsm_core::spec::Severity::Error);
            let matrix = completeness_matrix(&m.compiled, &t);
            let mut flist = Vec::new();
            for f in &findings {
                let mut o = BTreeMap::new();
                o.insert("code".into(), Value::Str(f.code.into()));
                o.insert(
                    "severity".into(),
                    Value::Str(
                        match f.severity {
                            fsm_core::spec::Severity::Error => "error",
                            fsm_core::spec::Severity::Warning => "warning",
                        }
                        .into(),
                    ),
                );
                o.insert("message".into(), Value::Str(f.message.clone()));
                o.insert("path".into(), Value::Str(f.path.clone()));
                o.insert("hint".into(), Value::Str(f.hint.clone()));
                flist.push(Value::Obj(o));
            }
            let mut cells = BTreeMap::new();
            for ((leaf, ev), cell) in &matrix {
                cells.insert(format!("{leaf}/{ev}"), Value::Str(cell.clone()));
            }
            let mut obj = BTreeMap::new();
            obj.insert("findings".into(), Value::Arr(flist));
            obj.insert("completeness".into(), Value::Obj(cells));
            emit_success(ctx, &Value::Obj(obj));
            if has_err { 1 } else { 0 }
        }
        Err(e) => emit_error(ctx, &e),
    }
}

pub static SPECS: &[CmdSpec] = &[
    CmdSpec {
        path: &["machine", "add"],
        positionals: &["spec"],
        flags: &["if-exists"],
        switches: &[],
        help: "Add a machine",
        run: add,
    },
    CmdSpec {
        path: &["machine", "ls"],
        positionals: &[],
        flags: &["name-contains"],
        switches: &[],
        help: "List machines",
        run: ls,
    },
    CmdSpec {
        path: &["machine", "show"],
        positionals: &["machine"],
        flags: &[],
        switches: &[],
        help: "Show a machine",
        run: show,
    },
    CmdSpec {
        path: &["machine", "analyze"],
        positionals: &["machine"],
        flags: &[],
        switches: &[],
        help: "Analyze a machine",
        run: analyze,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Ctx;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmp() -> std::path::PathBuf {
        let n = N.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("fsm-mc-{}-{n}", std::process::id()));
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

    fn ctx(dir: &std::path::Path) -> Ctx {
        Ctx::new(dir.to_path_buf(), true, false)
    }

    #[test]
    fn idempotent_add_and_strict() {
        let dir = tmp();
        let mut c = ctx(&dir);
        let args = Args {
            positionals: vec![case()],
            flags: BTreeMap::new(),
            switches: Default::default(),
        };
        assert_eq!(add(&mut c, &args), 0);
        assert_eq!(add(&mut c, &args), 0);
        let mut c2 = ctx(&dir);
        let args2 = Args {
            positionals: vec![case()],
            flags: BTreeMap::from([("if-exists".into(), "error".into())]),
            switches: Default::default(),
        };
        assert_eq!(add(&mut c2, &args2), 1);
    }

    #[test]
    fn ls_and_filter() {
        let dir = tmp();
        let mut c = ctx(&dir);
        add(
            &mut c,
            &Args {
                positionals: vec![case()],
                flags: BTreeMap::new(),
                switches: Default::default(),
            },
        );
        assert_eq!(
            ls(
                &mut c,
                &Args {
                    positionals: vec![],
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
                    flags: BTreeMap::from([("name-contains".into(), "case".into())]),
                    switches: Default::default()
                }
            ),
            0
        );
    }

    #[test]
    fn show_ambiguity_and_prefix() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let a = parse(
            std::fs::read(case()).unwrap().as_slice(),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        let d1 = store.define_machine(a.clone(), false, false).unwrap();
        let mut b = a.clone();
        if let Value::Obj(o) = &mut b {
            o.insert("description".into(), Value::Str("v2".into()));
        }
        store.define_machine(b, false, false).unwrap();
        drop(store);
        let mut c = ctx(&dir);
        assert_eq!(
            show(
                &mut c,
                &Args {
                    positionals: vec!["case_review".into()],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            1
        );
        let hex = d1.machine_id.split(':').next_back().unwrap()[..12].to_string();
        assert_eq!(
            show(
                &mut c,
                &Args {
                    positionals: vec![hex],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            0
        );
    }

    #[test]
    fn show_fidelity() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let def = parse(
            std::fs::read(case()).unwrap().as_slice(),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        store.define_machine(def.clone(), false, false).unwrap();
        let stored = store.resolve_machine("case_review").unwrap();
        assert_eq!(canon_bytes(&stored.def), canon_bytes(&def));
    }

    #[test]
    fn analyze_unreachable_and_shadowed() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let ghost = r#"{
          "format":"fsm.machine/1","name":"ghosty",
          "context":[],"events":[{"name":"go","fields":[]}],
          "states":[{"name":"a"},{"name":"ghost"}],
          "initial":"a",
          "transitions":[{"from":"a","on":"go","to":"a"}]
        }"#;
        let v = parse(ghost.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        store.define_machine(v, false, false).unwrap();
        drop(store);
        let mut c = ctx(&dir);
        let code = analyze(
            &mut c,
            &Args {
                positionals: vec!["ghosty".into()],
                flags: BTreeMap::new(),
                switches: Default::default(),
            },
        );
        assert_eq!(code, 0);
        let shadowed = r#"{
          "format":"fsm.machine/1","name":"shad",
          "context":[],"events":[{"name":"go","fields":[]}],
          "states":[{"name":"a"},{"name":"b","terminal":true}],
          "initial":"a",
          "transitions":[{"from":"a","on":"go","to":"b"},{"from":"a","on":"go","to":"a"}]
        }"#;
        let mut store = Store::open(&dir).unwrap();
        let v = parse(shadowed.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        store.define_machine(v, false, false).unwrap();
        drop(store);
        let code = analyze(
            &mut c,
            &Args {
                positionals: vec!["shad".into()],
                flags: BTreeMap::new(),
                switches: Default::default(),
            },
        );
        assert_eq!(code, 1);
    }
}
