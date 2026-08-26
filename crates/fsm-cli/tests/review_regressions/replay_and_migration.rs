use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, fsm_bin, gate, tmp};

#[test]
fn versions_before_checkpoint_format_and_missing_marker_migrate() {
    let _g = gate();
    let dir = tmp("ver");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    drop(s);
    for marker in ["3", "4", "5"] {
        std::fs::write(dir.join("VERSION"), format!("{marker}\n")).unwrap();
        let s = match Store::open(&dir) {
            Ok(s) => s,
            Err(e) => panic!("VERSION {marker} should migrate: {e:?}"),
        };
        assert_eq!(
            std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            fsm_store::journal_io::STORE_VERSION
        );
        s.resolve_machine("case_review").unwrap();
        drop(s);
    }
    std::fs::remove_file(dir.join("VERSION")).unwrap();
    let s = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => panic!("missing VERSION should migrate: {e:?}"),
    };
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        fsm_store::journal_io::STORE_VERSION
    );
    s.resolve_machine("case_review").unwrap();
    drop(s);
    // One past the current format: a store written by a newer build is refused,
    // never migrated. Derived so this stays a *future* version after a bump.
    let future = (fsm_store::journal_io::STORE_VERSION
        .parse::<u32>()
        .expect("STORE_VERSION is numeric")
        + 1)
    .to_string();
    std::fs::write(dir.join("VERSION"), format!("{future}\n")).unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("VERSION {future} opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/version_mismatch");
    match fsm_cli::journal_io::init(&dir) {
        Err(fsm_cli::journal_io::JournalIoError::VersionMismatch { found }) if found == future => {}
        Err(e) => panic!("expected version mismatch, got {e:?}"),
        Ok(_) => panic!("init succeeded on refused VERSION"),
    }
}

#[test]
fn span_bearing_rejected_retry_keeps_span() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"ov","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"}],"initial":"a","transitions":[{"from":"a","on":"go","do":[{"target":"x","value":"ctx.x + 1"}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("span");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(v, false, false).unwrap();
    s.create_instance("ov", "i1", "c1", None).unwrap();
    let e1 = s
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "ov1", None)
        .unwrap_err();
    assert_eq!(e1.code, "run/action_error");
    assert_eq!(
        e1.details.get("cause").and_then(Value::as_str),
        Some("run/overflow")
    );
    assert_eq!(e1.span, Some((0, 9)));
    assert!(!e1.duplicate);
    let e_same = s
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "ov1", None)
        .unwrap_err();
    assert!(e_same.duplicate);
    assert_eq!(e_same.span, e1.span);
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let e2 = s2
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "ov1", None)
        .unwrap_err();
    assert!(e2.duplicate);
    assert_eq!(e2.span, e1.span);
    assert_eq!(
        e2.details.get("block").and_then(Value::as_str),
        Some("transition")
    );
}

#[test]
fn altered_rejection_details_fail_replay() {
    let _g = gate();
    let dir = tmp("alt");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let last = recs.last().unwrap().clone();
    assert_eq!(last.kind, fsm_core::record::RecordKind::EventRejected);
    let prev = recs[recs.len() - 2].hash.clone();
    let mut body = last.body.as_obj().cloned().unwrap();
    body.insert("details".into(), Value::Obj(BTreeMap::new()));
    let forged = fsm_core::record::seal(last.seq, last.ts, last.kind, Value::Obj(body), &prev);
    let jdir = dir.join("journal");
    let seg = std::fs::read_dir(&jdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("seg-"))
                .unwrap_or(false)
        })
        .unwrap();
    let mut lines: Vec<Vec<u8>> = std::fs::read(&seg)
        .unwrap()
        .split_inclusive(|&b| b == b'\n')
        .map(|l| l.to_vec())
        .filter(|l| l.iter().any(|b| !b.is_ascii_whitespace()))
        .collect();
    lines.pop();
    let mut out = Vec::new();
    for l in &lines {
        out.extend_from_slice(l);
        if !l.ends_with(&[b'\n']) {
            out.push(b'\n');
        }
    }
    out.extend_from_slice(&forged.to_line());
    out.push(b'\n');
    std::fs::write(&seg, out).unwrap();
    drop(s);
    let report = fsm_cli::journal_io::verify(&dir);
    assert!(
        !matches!(report.health, fsm_cli::journal_io::JournalHealth::Ok),
        "{:?}",
        report.health
    );
}

fn rewrite_last_record(dir: &std::path::Path, mut edit: impl FnMut(&mut BTreeMap<String, Value>)) {
    let recs = fsm_cli::journal_io::load_records(dir).unwrap();
    let last = recs.last().unwrap().clone();
    let prev = recs[recs.len() - 2].hash.clone();
    let mut body = last.body.as_obj().cloned().unwrap();
    edit(&mut body);
    let forged = fsm_core::record::seal(last.seq, last.ts, last.kind, Value::Obj(body), &prev);
    let jdir = dir.join("journal");
    let seg = std::fs::read_dir(&jdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("seg-"))
                .unwrap_or(false)
        })
        .unwrap();
    let mut lines: Vec<Vec<u8>> = std::fs::read(&seg)
        .unwrap()
        .split_inclusive(|&b| b == b'\n')
        .map(|l| l.to_vec())
        .filter(|l| l.iter().any(|b| !b.is_ascii_whitespace()))
        .collect();
    lines.pop();
    let mut out = Vec::new();
    for l in &lines {
        out.extend_from_slice(l);
        if !l.ends_with(&[b'\n']) {
            out.push(b'\n');
        }
    }
    out.extend_from_slice(&forged.to_line());
    out.push(b'\n');
    std::fs::write(&seg, out).unwrap();
}

fn verify_not_ok(dir: &std::path::Path) {
    let report = fsm_cli::journal_io::verify(dir);
    assert!(
        !matches!(report.health, fsm_cli::journal_io::JournalHealth::Ok),
        "{:?}",
        report.health
    );
}

#[test]
fn extra_key_event_rejected_fails_replay() {
    let _g = gate();
    let dir = tmp("xkey");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    drop(s);
    rewrite_last_record(&dir, |body| {
        let mut d = body
            .get("details")
            .and_then(Value::as_obj)
            .cloned()
            .unwrap_or_default();
        d.insert("fabricated".into(), Value::Str("accepted".into()));
        body.insert("details".into(), Value::Obj(d));
    });
    verify_not_ok(&dir);
}

#[test]
fn unexpected_block_event_rejected_fails_replay() {
    let _g = gate();
    let dir = tmp("xblk");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    drop(s);
    rewrite_last_record(&dir, |body| {
        let mut d = body
            .get("details")
            .and_then(Value::as_obj)
            .cloned()
            .unwrap_or_default();
        d.insert("block".into(), Value::Str("nope".into()));
        body.insert("details".into(), Value::Obj(d));
    });
    verify_not_ok(&dir);
}

#[test]
fn extra_key_and_span_request_rejected_fails_replay() {
    let _g = gate();
    let dir = tmp("xrr");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.ack_effect_outcome("i1", "missing", "ar", "ok", None)
        .unwrap_err();
    drop(s);
    rewrite_last_record(&dir, |body| {
        let mut d = body
            .get("details")
            .and_then(Value::as_obj)
            .cloned()
            .unwrap_or_default();
        d.insert("fabricated".into(), Value::Str("accepted".into()));
        body.insert("details".into(), Value::Obj(d));
        let mut sp = BTreeMap::new();
        sp.insert("start".into(), Value::Num("1".into()));
        sp.insert("end".into(), Value::Num("2".into()));
        body.insert("span".into(), Value::Obj(sp));
    });
    verify_not_ok(&dir);
}

#[test]
fn lock_held_store_open_writes_no_version() {
    let _g = gate();
    let dir = tmp("lockv");
    std::fs::create_dir_all(dir.join("journal")).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(dir.join("journal/LOCK"))
        .unwrap();
    lock.try_lock().unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("open succeeded while locked"),
        Err(e) => e,
    };
    assert!(
        err.code.contains("lock") || err.message.contains("lock"),
        "{err:?}"
    );
    assert!(!dir.join("VERSION").exists());
}

#[test]
fn concurrent_first_open_installs_one_version() {
    let _g = gate();
    let dir = tmp("conc");
    let a = dir.to_path_buf();
    let b = dir.to_path_buf();
    let t1 = std::thread::spawn(move || Store::open(&a));
    let t2 = std::thread::spawn(move || Store::open(&b));
    let r1 = t1.join().unwrap();
    let r2 = t2.join().unwrap();
    assert!(r1.is_ok() || r2.is_ok(), "both first opens failed");
    drop(r1);
    drop(r2);
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION"))
            .unwrap_or_default()
            .trim(),
        fsm_store::journal_io::STORE_VERSION
    );
}

#[test]
fn old_snapshot_format_rejected() {
    let v = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.snapshot/1".into())),
        ("seq".into(), Value::Num("1".into())),
    ]));
    assert!(fsm_cli::snapshot::snapshot_to_state(&v).is_err());
}

#[test]
fn migration_ignores_snapshot_caches() {
    let _g = gate();
    let dir = tmp("migsnap");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.shutdown_snapshot().unwrap();
    drop(s);
    let s = Store::open(&dir).unwrap();
    assert!(
        s.opened_from_snapshot,
        "bound snapshot must fast-path a current store"
    );
    drop(s);
    std::fs::write(dir.join("VERSION"), "5\n").unwrap();
    let s = Store::open(&dir).unwrap();
    assert!(
        !s.opened_from_snapshot,
        "migration must fold the complete journal"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        fsm_store::journal_io::STORE_VERSION
    );
    s.resolve_machine("case_review").unwrap();
    drop(s);
    let s = Store::open(&dir).unwrap();
    assert!(
        s.opened_from_snapshot,
        "stamped store returns to the fast path"
    );
    drop(s);
}

#[test]
fn migratable_marker_with_lost_journal_refuses() {
    let _g = gate();
    let dir = tmp("miglost");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("VERSION"), "4\n").unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("lost-journal migratable dir opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/chain_broken");
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        "4"
    );
}

#[test]
fn verify_report_ordered_segment_progress() {
    let _g = gate();
    let dir = tmp("segs");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    for i in 0..3 {
        store
            .create_instance("case_review", &format!("i{i}"), &format!("c{i}"), None)
            .unwrap();
    }
    let first_name = store.journal.seg_name.clone();
    store.journal.force_rotate().unwrap();
    for i in 3..6 {
        store
            .create_instance("case_review", &format!("i{i}"), &format!("c{i}"), None)
            .unwrap();
    }
    let second_name = store.journal.seg_name.clone();
    assert_ne!(first_name, second_name, "rotation must open a new segment");
    drop(store);
    let bin = fsm_bin();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "verify",
            "--report",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("\"records\""), "{stdout}");
    assert!(stdout.contains("\"status\""), "{stdout}");
    assert!(stdout.contains(&first_name), "{stdout}");
    assert!(stdout.contains(&second_name), "{stdout}");
    let v = parse(stdout.trim().as_bytes(), &JsonLimits::DEFAULT).expect(&stdout);
    let segs = v.get("segments").and_then(Value::as_arr).expect(&stdout);
    let reported: Vec<&str> = segs
        .iter()
        .map(|s| s.get("segment").and_then(Value::as_str).unwrap_or(""))
        .collect();
    let mut sorted = reported.clone();
    sorted.sort();
    assert_eq!(reported, sorted, "CLI segments unordered {stdout}");

    let bogus = dir.join("journal").join("seg-zzzz.jsonl");
    std::fs::create_dir_all(&bogus).unwrap();
    let r = fsm_cli::journal_io::verify(&dir);
    assert!(
        matches!(r.health, fsm_cli::journal_io::JournalHealth::StoreIo(_)),
        "non-regular segment must be a fatal read error: {:?}",
        r.health
    );
    assert!(
        r.segments.len() >= 3,
        "expected ≥2 real segments plus metadata-failure, got {:?}",
        r.segments
            .iter()
            .map(|s| format!("{}:{}", s.segment, s.status))
            .collect::<Vec<_>>()
    );
    let names: Vec<_> = r.segments.iter().map(|s| s.segment.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "segments must be ordered");
    assert!(
        r.segments
            .iter()
            .filter(|s| s.status == "ok" && s.records > 0)
            .count()
            >= 2,
        "need two populated segments {:?}",
        r.segments
            .iter()
            .map(|s| format!("{}:{}:{}", s.segment, s.status, s.records))
            .collect::<Vec<_>>()
    );
    assert!(
        r.segments.iter().any(|s| s.status == "metadata-failure"),
        "missing metadata-failure {:?}",
        r.segments
            .iter()
            .map(|s| s.status.as_str())
            .collect::<Vec<_>>()
    );

    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "verify",
            "--report",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(out.stdout.is_empty());
    let error = parse(&out.stderr, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(error.get("code").and_then(Value::as_str), Some("io/read"));
}
