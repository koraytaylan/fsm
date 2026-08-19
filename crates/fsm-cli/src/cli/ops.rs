use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_core::replay::{NopSink, fold_with};

use crate::args::{Args, CmdSpec, Ctx};
use crate::journal_io::{
    DetectedStoreFormat, JournalHealth, RepairError, detect_store_format, load_records,
    repair_truncate_torn_tail, verify,
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
        JournalHealth::StoreIo(_) => 6,
    }
}

fn store_format_refusal_code(h: &JournalHealth) -> &'static str {
    match h {
        JournalHealth::StoreIo(_) => "io/read",
        _ => "store/version_mismatch",
    }
}

fn journal_verify(ctx: &mut Ctx, args: &Args) -> u8 {
    let r = verify(&ctx.data_dir);
    if matches!(
        r.health,
        JournalHealth::VersionMismatch { .. } | JournalHealth::StoreIo(_)
    ) {
        let mut error = ErrorObj::new(store_format_refusal_code(&r.health), r.health.message());
        if matches!(r.health, JournalHealth::StoreIo(_)) {
            error = error.hint("restore the named persistence path as a readable regular file or directory within the documented per-unit limit, then retry");
        }
        return emit_error(ctx, &error);
    }
    let mut m = BTreeMap::new();
    m.insert("records".into(), Value::Num(r.records.to_string()));
    m.insert("machines".into(), Value::Num(r.machines.to_string()));
    m.insert("instances".into(), Value::Num(r.instances.to_string()));
    m.insert("health".into(), Value::Str(format!("{:?}", r.health)));
    if let Some(v) = &r.store_version {
        m.insert("store_version".into(), Value::Str(v.clone()));
    }
    if r.migratable {
        m.insert("migratable".into(), Value::Bool(true));
    }
    if args.switches.contains("report") {
        m.insert("report".into(), Value::Bool(true));
        let hashes: Vec<Value> = r
            .instance_hashes
            .iter()
            .map(|(id, h)| {
                Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str(id.clone())),
                    ("state_hash".into(), Value::Str(h.clone())),
                ]))
            })
            .collect();
        m.insert("instance_hashes".into(), Value::Arr(hashes));
        let segs: Vec<Value> = r
            .segments
            .iter()
            .map(|s| {
                let mut o = BTreeMap::from([
                    ("segment".into(), Value::Str(s.segment.clone())),
                    ("records".into(), Value::Num(s.records.to_string())),
                    ("status".into(), Value::Str(s.status.clone())),
                ]);
                if let Some(n) = s.first_seq {
                    o.insert("first_seq".into(), Value::Num(n.to_string()));
                }
                if let Some(n) = s.last_seq {
                    o.insert("last_seq".into(), Value::Num(n.to_string()));
                }
                Value::Obj(o)
            })
            .collect();
        m.insert("segments".into(), Value::Arr(segs));
    }
    emit_success(ctx, &Value::Obj(m));
    health_exit(&r.health)
}

fn journal_replay(ctx: &mut Ctx, args: &Args) -> u8 {
    if let Err(h) = crate::journal_io::refuse_incompatible_store_format(&ctx.data_dir) {
        return emit_error(
            ctx,
            &ErrorObj::new(store_format_refusal_code(&h), h.message()),
        );
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
    let last = recs.last().map(|r| r.seq).unwrap_or(0);
    if let Some(n) = to {
        if n > last {
            emit_success(
                ctx,
                &Value::Obj(BTreeMap::from([
                    ("agreement".into(), Value::Bool(false)),
                    (
                        "first_divergent_seq".into(),
                        Value::Num((last + 1).to_string()),
                    ),
                ])),
            );
            return 1;
        }
    }
    let recs: Vec<_> = recs
        .into_iter()
        .filter(|r| to.map(|n| r.seq <= n).unwrap_or(true))
        .collect();
    match fold_with(recs.clone(), &mut NopSink) {
        Ok(folded) => {
            if matches!(
                detect_store_format(&ctx.data_dir),
                DetectedStoreFormat::Migratable { .. }
            ) {
                // Migration ignores snapshot caches wholesale, so against a
                // migratable store the complete fold is the whole verdict;
                // judging pre-migration caches would only manufacture alarms.
                emit_success(
                    ctx,
                    &Value::Obj(BTreeMap::from([
                        ("agreement".into(), Value::Bool(true)),
                        ("snapshots_ignored".into(), Value::Bool(true)),
                    ])),
                );
                return 0;
            }
            let live_at = match crate::snapshot::reconstruct_snapshot_plus_tail(
                &ctx.data_dir,
                &recs,
                folded.last_seq,
            ) {
                Ok(s) => s,
                Err(_) => {
                    let div = first_divergent_view(&recs, &ctx.data_dir).unwrap_or(folded.last_seq);
                    emit_success(
                        ctx,
                        &Value::Obj(BTreeMap::from([
                            ("agreement".into(), Value::Bool(false)),
                            ("first_divergent_seq".into(), Value::Num(div.to_string())),
                        ])),
                    );
                    return 1;
                }
            };
            let agreement = states_agree(&folded, &live_at);
            let mut out = BTreeMap::from([("agreement".into(), Value::Bool(agreement))]);
            if !agreement {
                let div = first_divergent_view(&recs, &ctx.data_dir)
                    .unwrap_or_else(|| folded.last_seq.min(live_at.last_seq).saturating_add(1));
                out.insert("first_divergent_seq".into(), Value::Num(div.to_string()));
            }
            emit_success(ctx, &Value::Obj(out));
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
    let format = detect_store_format(&ctx.data_dir);
    match crate::store::Store::open_read_only(&ctx.data_dir) {
        Ok(_) => {
            m.insert("readable".into(), Value::Bool(true));
        }
        Err(error) => {
            return emit_error(ctx, &error);
        }
    }
    let ver = match &format {
        DetectedStoreFormat::Current => crate::journal_io::STORE_VERSION.to_string(),
        DetectedStoreFormat::Migratable { found } | DetectedStoreFormat::Incompatible { found } => {
            found.clone()
        }
        DetectedStoreFormat::Empty | DetectedStoreFormat::Unreadable { .. } => String::new(),
    };
    m.insert("version".into(), Value::Str(ver.clone()));
    if let DetectedStoreFormat::Migratable { found } = &format {
        m.insert("migration_required_from".into(), Value::Str(found.clone()));
    }
    let snaps = crate::snapshot::listed_snaps(&ctx.data_dir).len();
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
                JournalHealth::StoreIo(_) => "io/read",
                JournalHealth::LockIo(_) => "store/lock",
                JournalHealth::TornTail { .. } => "store/torn_tail",
                _ => "store/chain_broken",
            };
            let mut error = ErrorObj::new(code, h.message());
            if matches!(h, JournalHealth::StoreIo(_)) {
                error = error.hint("restore the named persistence path as a readable regular file or directory within the documented per-unit limit, then retry");
            }
            emit_error(ctx, &error)
        }
        Err(RepairError::ReadIo(message)) => emit_error(ctx, &ErrorObj::new("io/read", message)),
        Err(RepairError::WriteIo(message)) => emit_error(ctx, &ErrorObj::new("io/write", message)),
        Err(RepairError::NothingToRepair) => {
            emit_error(ctx, &ErrorObj::new("store/torn_tail", "NothingToRepair"))
        }
    }
}

#[allow(dead_code)]
fn first_divergent_seq(
    journal: &[fsm_core::record::Record],
    live_recs: &[fsm_core::record::Record],
) -> Option<u64> {
    use fsm_core::replay::{NopSink, fold_with};
    let max = journal
        .last()
        .map(|r| r.seq)
        .unwrap_or(0)
        .max(live_recs.last().map(|r| r.seq).unwrap_or(0));
    for seq in 1..=max {
        let jp: Vec<_> = journal.iter().filter(|r| r.seq <= seq).cloned().collect();
        let lp: Vec<_> = live_recs.iter().filter(|r| r.seq <= seq).cloned().collect();
        let Ok(jf) = fold_with(jp, &mut NopSink) else {
            return Some(seq);
        };
        let Ok(lf) = fold_with(lp, &mut NopSink) else {
            return Some(seq);
        };
        if !states_agree(&jf, &lf) {
            return Some(seq);
        }
    }
    None
}

fn first_divergent_view(
    journal: &[fsm_core::record::Record],
    data_dir: &std::path::Path,
) -> Option<u64> {
    use fsm_core::replay::{NopSink, fold_with};
    let max = journal.last().map(|r| r.seq).unwrap_or(0);
    for seq in 1..=max {
        let jp: Vec<_> = journal.iter().filter(|r| r.seq <= seq).cloned().collect();
        let Ok(jf) = fold_with(jp, &mut NopSink) else {
            return Some(seq);
        };
        let Ok(live) = crate::snapshot::reconstruct_snapshot_plus_tail(data_dir, journal, seq)
        else {
            return Some(seq);
        };
        if !states_agree(&jf, &live) {
            return Some(seq);
        }
    }
    None
}

fn states_agree(a: &fsm_core::replay::StoreState, b: &fsm_core::replay::StoreState) -> bool {
    if a.last_seq != b.last_seq || a.last_hash != b.last_hash {
        return false;
    }
    if a.machines.len() != b.machines.len() {
        return false;
    }
    for (id, ma) in &a.machines {
        let Some(mb) = b.machines.get(id) else {
            return false;
        };
        if ma.def != mb.def || ma.compiled.machine_id != mb.compiled.machine_id {
            return false;
        }
    }
    if a.dedup != b.dedup {
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
        if ia.configuration != ib.configuration
            || ia.status != ib.status
            || ia.ctx != ib.ctx
            || ia.history != ib.history
            || ia.deadlines != ib.deadlines
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

    #[test]
    fn first_divergent_early_middle_final() {
        let dir = tmp();
        let _ = crate::journal_io::init(&dir);
        let mut store = crate::store::Store::open(&dir).unwrap();
        let spec = fsm_core::json::parse(
            include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        store.define_machine(spec, false, false).unwrap();
        store
            .create_instance("case_review", "i1", "c1", None)
            .unwrap();
        store
            .send_event("i1", "docs_ok", Value::Obj(Default::default()), "s1", None)
            .unwrap();
        let recs = store.records.clone();
        drop(store);
        assert_eq!(first_divergent_seq(&recs, &recs), None);
        let early = recs[1].seq;
        let mid = recs[recs.len() / 2].seq;
        let last = recs.last().unwrap().seq;
        for cut in [early, mid, last] {
            let live: Vec<_> = recs.iter().filter(|r| r.seq < cut).cloned().collect();
            let d = first_divergent_seq(&recs, &live).expect("div");
            assert_eq!(d, cut, "cut {cut}");
        }
    }
}
