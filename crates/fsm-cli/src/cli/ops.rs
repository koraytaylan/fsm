use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_core::replay::{NopSink, fold_with};

use crate::args::{Args, CmdSpec, Ctx};
use crate::journal_io::{
    JournalHealth, RepairError, load_records, repair_truncate_torn_tail, verify,
};
use crate::render::{emit_error, emit_success};
use crate::store::ErrorObj;

pub fn health_exit(h: &JournalHealth) -> u8 {
    match h {
        JournalHealth::Ok => 0,
        JournalHealth::TornTail { .. } => 2,
        JournalHealth::ChainBroken { .. } => 3,
        JournalHealth::StateHashMismatch { .. } => 4,
        JournalHealth::NonCanonical { .. } => 5,
        JournalHealth::LockIo(_) => 6,
        JournalHealth::ReplayMismatch { .. } => 4,
        JournalHealth::MissingGenesis => 3,
        JournalHealth::VersionMismatch { .. } => 6,
    }
}

fn journal_verify(ctx: &mut Ctx, args: &Args) -> u8 {
    let r = verify(&ctx.data_dir);
    let mut m = BTreeMap::new();
    m.insert("records".into(), Value::Num(r.records.to_string()));
    m.insert("machines".into(), Value::Num(r.machines.to_string()));
    m.insert("instances".into(), Value::Num(r.instances.to_string()));
    m.insert("health".into(), Value::Str(format!("{:?}", r.health)));
    if args.switches.contains("report") {
        m.insert("report".into(), Value::Bool(true));
    }
    emit_success(ctx, &Value::Obj(m));
    health_exit(&r.health)
}

fn journal_replay(ctx: &mut Ctx, args: &Args) -> u8 {
    if let Err(h) = crate::journal_io::require_store_format(&ctx.data_dir) {
        return emit_error(ctx, &ErrorObj::new("store/version_mismatch", h.message()));
    }
    let recs = match load_records(&ctx.data_dir) {
        Ok(r) => r,
        Err(e) => return emit_error(ctx, &ErrorObj::new("io/read", e)),
    };
    let to = match args.flags.get("to-seq") {
        None => None,
        Some(s) => match s.parse::<u64>() {
            Ok(n) => Some(n),
            Err(_) => {
                return emit_error(
                    ctx,
                    &ErrorObj::new("args", "to-seq must be a u64").hint("pass an integer sequence"),
                );
            }
        },
    };
    let recs: Vec<_> = recs
        .into_iter()
        .filter(|r| to.map(|n| r.seq <= n).unwrap_or(true))
        .collect();
    match fold_with(recs, &mut NopSink) {
        Ok(folded) => {
            let live = match crate::store::Store::open(&ctx.data_dir) {
                Ok(s) => s,
                Err(e) => return emit_error(ctx, &e),
            };
            let agreement = states_agree(&folded, &live.state);
            emit_success(
                ctx,
                &Value::Obj(BTreeMap::from([(
                    "agreement".into(),
                    Value::Bool(agreement),
                )])),
            );
            if agreement { 0 } else { 1 }
        }
        Err(e) => emit_error(
            ctx,
            &ErrorObj::new("store/state_hash_mismatch", format!("{e:?}")),
        ),
    }
}

fn doctor(ctx: &mut Ctx, _args: &Args) -> u8 {
    let mut m = BTreeMap::new();
    m.insert(
        "data_dir".into(),
        Value::Str(ctx.data_dir.display().to_string()),
    );
    let ver = std::fs::read_to_string(ctx.data_dir.join("VERSION")).unwrap_or_default();
    m.insert("version".into(), Value::Str(ver.trim().into()));
    let snaps = std::fs::read_dir(ctx.data_dir.join("snapshots"))
        .map(|rd| rd.count())
        .unwrap_or(0);
    m.insert("snapshots".into(), Value::Num(snaps.to_string()));
    let v = verify(&ctx.data_dir);
    m.insert("verify".into(), Value::Str(format!("{:?}", v.health)));
    m.insert(
        "FSM_DATA_DIR".into(),
        Value::Str(std::env::var("FSM_DATA_DIR").unwrap_or_default()),
    );
    m.insert(
        "FSM_LOG".into(),
        Value::Str(std::env::var("FSM_LOG").unwrap_or_default()),
    );
    if let Ok(s) = crate::store::Store::open(&ctx.data_dir) {
        m.insert(
            "lock_holder".into(),
            Value::Str(format!("{}", std::process::id())),
        );
        drop(s);
    }
    emit_success(ctx, &Value::Obj(m));
    0
}

fn repair(ctx: &mut Ctx, args: &Args) -> u8 {
    if !args.switches.contains("truncate-torn-tail") {
        return emit_error(ctx, &ErrorObj::new("args", "repair --truncate-torn-tail"));
    }
    match repair_truncate_torn_tail(&ctx.data_dir) {
        Ok(r) => {
            let mut m = BTreeMap::new();
            m.insert(
                "quarantine".into(),
                Value::Str(r.quarantined.display().to_string()),
            );
            m.insert(
                "truncated_to_seq".into(),
                Value::Num(r.truncated_to_seq.to_string()),
            );
            emit_success(ctx, &Value::Obj(m));
            0
        }
        Err(RepairError::Interior(h)) => {
            let code = match h {
                JournalHealth::VersionMismatch { .. } => "store/version_mismatch",
                JournalHealth::TornTail { .. } => "store/torn_tail",
                _ => "store/chain_broken",
            };
            emit_error(ctx, &ErrorObj::new(code, h.message()));
            health_exit(&h)
        }
        Err(e) => emit_error(ctx, &ErrorObj::new("store/torn_tail", format!("{e:?}"))),
    }
}

fn states_agree(a: &fsm_core::replay::StoreState, b: &fsm_core::replay::StoreState) -> bool {
    if a.last_seq != b.last_seq || a.last_hash != b.last_hash {
        return false;
    }
    if a.machines.keys().ne(b.machines.keys()) {
        return false;
    }
    if a.instance_machines != b.instance_machines {
        return false;
    }
    if a.instances.len() != b.instances.len() {
        return false;
    }
    for (id, ia) in &a.instances {
        let Some(ib) = b.instances.get(id) else {
            return false;
        };
        if ia.leaf != ib.leaf
            || ia.status != ib.status
            || ia.ctx != ib.ctx
            || ia.history != ib.history
            || ia.pending != ib.pending
        {
            return false;
        }
    }
    true
}

pub static SPECS: &[CmdSpec] = &[
    CmdSpec {
        path: &["journal", "verify"],
        positionals: &[],
        flags: &[],
        switches: &["report"],
        help: "Verify journal",
        run: journal_verify,
    },
    CmdSpec {
        path: &["journal", "replay"],
        positionals: &[],
        flags: &["to-seq"],
        switches: &[],
        help: "Replay journal",
        run: journal_replay,
    },
    CmdSpec {
        path: &["doctor"],
        positionals: &[],
        flags: &[],
        switches: &[],
        help: "Doctor the data dir",
        run: doctor,
    },
    CmdSpec {
        path: &["repair"],
        positionals: &[],
        flags: &[],
        switches: &["truncate-torn-tail"],
        help: "Repair torn tail",
        run: repair,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Ctx;
    use crate::journal_io::{JournalHealth, init};
    use fsm_core::json::Value;
    use fsm_core::record::RecordKind;
    use std::collections::BTreeSet;

    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "fsm-ops-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn clean() -> std::path::PathBuf {
        let dir = tmp();
        let _j = init(&dir).unwrap();
        dir
    }

    #[test]
    fn health_exit_map() {
        assert_eq!(health_exit(&JournalHealth::Ok), 0);
        assert_eq!(
            health_exit(&JournalHealth::TornTail {
                segment: "s".into(),
                offset: 0,
                bytes: 1
            }),
            2
        );
        assert_eq!(
            health_exit(&JournalHealth::ChainBroken {
                seq: 1,
                segment: "s".into(),
                offset: 0,
                expected: "a".into(),
                found: "b".into()
            }),
            3
        );
        assert_eq!(health_exit(&JournalHealth::StateHashMismatch { seq: 1 }), 4);
        assert_eq!(
            health_exit(&JournalHealth::NonCanonical {
                seq: 1,
                segment: "s".into(),
                offset: 0
            }),
            5
        );
        assert_eq!(health_exit(&JournalHealth::LockIo("x".into())), 6);
        assert_eq!(
            health_exit(&JournalHealth::VersionMismatch { found: "1".into() }),
            6
        );
    }

    #[test]
    fn verify_clean_and_torn() {
        let dir = clean();
        let mut c = Ctx::new(dir.clone(), true, false);
        assert_eq!(
            journal_verify(
                &mut c,
                &Args {
                    positionals: vec![],
                    flags: BTreeMap::new(),
                    switches: BTreeSet::from(["report"])
                }
            ),
            0
        );
        let seg = dir.join("journal/seg-00000000000000000000.jsonl");
        let mut bytes = std::fs::read(&seg).unwrap();
        bytes.truncate(bytes.len() - 3);
        std::fs::write(&seg, &bytes).unwrap();
        assert_eq!(
            journal_verify(
                &mut c,
                &Args {
                    positionals: vec![],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            2
        );
    }

    #[test]
    fn replay_and_doctor() {
        let dir = clean();
        let mut c = Ctx::new(dir.clone(), true, false);
        assert_eq!(
            journal_replay(
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
            doctor(
                &mut c,
                &Args {
                    positionals: vec![],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            0
        );
    }

    #[test]
    fn repair_torn_and_refuse_interior() {
        let dir = clean();
        let seg = dir.join("journal/seg-00000000000000000000.jsonl");
        let mut bytes = std::fs::read(&seg).unwrap();
        bytes.truncate(bytes.len() - 3);
        std::fs::write(&seg, &bytes).unwrap();
        let mut c = Ctx::new(dir.clone(), true, false);
        assert_eq!(
            repair(
                &mut c,
                &Args {
                    positionals: vec![],
                    flags: BTreeMap::new(),
                    switches: BTreeSet::from(["truncate-torn-tail"])
                }
            ),
            0
        );
        assert_eq!(
            journal_verify(
                &mut c,
                &Args {
                    positionals: vec![],
                    flags: BTreeMap::new(),
                    switches: Default::default()
                }
            ),
            0
        );

        let dir2 = clean();
        let mut j = init(&dir2).unwrap();
        let mut b = BTreeMap::new();
        b.insert("instance_id".into(), Value::Str("j".into()));
        j.append(RecordKind::Annotated, Value::Obj(b)).unwrap();
        drop(j);
        let seg = dir2.join("journal/seg-00000000000000000000.jsonl");
        let mut bytes = std::fs::read(&seg).unwrap();
        if let Some(pos) = bytes.iter().position(|&b| b == b'{') {
            bytes[pos + 2] ^= 0x01;
        }
        std::fs::write(&seg, &bytes).unwrap();
        let mut c2 = Ctx::new(dir2, true, false);
        let code = repair(
            &mut c2,
            &Args {
                positionals: vec![],
                flags: BTreeMap::new(),
                switches: BTreeSet::from(["truncate-torn-tail"]),
            },
        );
        assert_ne!(code, 0);
    }
}
