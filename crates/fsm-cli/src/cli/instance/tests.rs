//! The instance commands' own tests.

// The commands under test, and the argument shapes they take.
use super::{Args, ack, annotate, cancel, explain, history, ls, new_inst, send, show};
use crate::args::Ctx;
use crate::cli::machine;
use crate::clock;
use crate::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

/// A scratch directory that removes itself. A suite that leaks one per run
/// exhausts a long-lived machine's tmpfs inodes long before it exhausts its
/// bytes, and the failure looks like a broken toolchain rather than a leaky
/// test.
struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp() -> Scratch {
    let n = N.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("fsm-ic-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn case() -> String {
    format!(
        "{}/../fsm-core/tests/fixtures/machines/case_review.json",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn setup() -> (Scratch, String) {
    clock::reset_injected();
    clock::force_ms(5_000);
    crate::args::reset_request_ids();
    let dir = tmp();
    let mut c = Ctx::new(dir.to_path_buf(), true, false);
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
    let mut c = Ctx::new(dir.to_path_buf(), true, false);
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
    let mut c = Ctx::new(dir.to_path_buf(), true, false);
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
    let mut c = Ctx::new(dir.to_path_buf(), true, false);
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
    let mut c = Ctx::new(dir.to_path_buf(), true, false);
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
    let mut c = Ctx::new(dir2.to_path_buf(), true, false);
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
