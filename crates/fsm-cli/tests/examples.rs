use fsm_cli::clock::FixedClock;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::{compile, parse_machine};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Keeps concurrently created test directories distinct within this process.
static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-cli-examples-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn load(name: &str) -> Value {
    let p = format!("{}/../../examples/{name}.json", env!("CARGO_MANIFEST_DIR"));
    parse(&std::fs::read(p).unwrap(), &JsonLimits::DEFAULT).unwrap()
}

fn valid(name: &str) {
    let v = load(name);
    let spec = parse_machine(&v).unwrap_or_else(|e| panic!("{name} {e:?}"));
    compile(spec).unwrap_or_else(|e| panic!("{name} {e:?}"));
}

#[test]
fn all_valid() {
    valid("expense_approval");
    valid("order_lifecycle");
    valid("invoice_matching");
    valid("parallel_review_deadline");
}

#[test]
fn parallel_review_deadline_is_explicit_and_region_safe() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    store
        .define_machine(load("parallel_review_deadline"), false, false)
        .unwrap();

    let mut create_clock = FixedClock::new(1_000, 1);
    let created = store
        .create_instance_ctx_on(
            &mut create_clock,
            "parallel_review_deadline",
            "parallel",
            "parallel-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    assert!(
        created.get("leaf").is_none(),
        "parallel views have no fake leaf"
    );
    let leaves = created
        .get("configuration")
        .and_then(|configuration| configuration.get("leaves"))
        .and_then(Value::as_obj)
        .unwrap();
    assert_eq!(
        leaves.get("review").and_then(Value::as_str),
        Some("awaiting_review")
    );
    assert_eq!(
        leaves.get("audit").and_then(Value::as_str),
        Some("auditing")
    );
    assert_eq!(
        store
            .state
            .instances
            .get("parallel")
            .unwrap()
            .deadlines
            .get("review_timeout"),
        Some(&31_000)
    );

    let mut early_clock = FixedClock::new(30_999, 1);
    let early = store
        .poll_instance_deadline_on(&mut early_clock, "parallel", "poll-early", None)
        .unwrap();
    assert_eq!(
        early.get("deadline_not_due").and_then(Value::as_bool),
        Some(true)
    );

    let mut due_clock = FixedClock::new(31_000, 1);
    let fired = store
        .poll_instance_deadline_on(&mut due_clock, "parallel", "poll-due", None)
        .unwrap();
    assert_eq!(
        fired.get("deadline").and_then(Value::as_str),
        Some("review_timeout")
    );
    let leaves = fired
        .get("configuration")
        .and_then(|configuration| configuration.get("leaves"))
        .and_then(Value::as_obj)
        .unwrap();
    assert_eq!(
        leaves.get("review").and_then(Value::as_str),
        Some("timed_out")
    );
    assert_eq!(
        leaves.get("audit").and_then(Value::as_str),
        Some("auditing")
    );
    assert_eq!(fired.get("status").and_then(Value::as_str), Some("running"));

    let completed = store
        .send_event(
            "parallel",
            "audit_ok",
            Value::Obj(BTreeMap::new()),
            "audit-ok",
            None,
        )
        .unwrap();
    assert_eq!(
        completed.get("status").and_then(Value::as_str),
        Some("completed")
    );
}

#[test]
fn expense_approval_paths() {
    let directory = TestDirectory::create();
    let mut s = Store::open(directory.path()).unwrap();
    s.define_machine(load("expense_approval"), false, false)
        .unwrap();
    s.create_instance("expense_approval", "small", "c1", None)
        .unwrap();
    let r = s
        .send_event(
            "small",
            "submit",
            Value::Obj(BTreeMap::from([(
                "amount".into(),
                Value::Str("10.00".into()),
            )])),
            "s1",
            None,
        )
        .unwrap();
    assert_eq!(r.get("leaf").and_then(Value::as_str), Some("peer_review"));
    s.send_event("small", "approve", Value::Obj(BTreeMap::new()), "s2", None)
        .unwrap();
    assert_eq!(
        s.state
            .instances
            .get("small")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("approved")
    );

    s.create_instance("expense_approval", "ancestor", "c-ancestor", None)
        .unwrap();
    s.send_event(
        "ancestor",
        "submit",
        Value::Obj(BTreeMap::from([(
            "amount".into(),
            Value::Str("10.00".into()),
        )])),
        "ancestor-submit",
        None,
    )
    .unwrap();
    assert_eq!(
        s.state
            .instances
            .get("ancestor")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("peer_review")
    );
    let ancestor = s
        .send_event(
            "ancestor",
            "withdraw",
            Value::Obj(BTreeMap::new()),
            "ancestor-withdraw",
            None,
        )
        .unwrap();
    assert_eq!(ancestor.get("leaf").and_then(Value::as_str), Some("draft"));
    assert_eq!(
        ancestor
            .get("transition")
            .and_then(|t| t.get("source_state"))
            .and_then(Value::as_str),
        Some("review"),
        "peer_review must inherit the ancestor-sourced withdraw"
    );

    s.create_instance("expense_approval", "big", "c2", None)
        .unwrap();
    let r = s
        .send_event(
            "big",
            "submit",
            Value::Obj(BTreeMap::from([(
                "amount".into(),
                Value::Str("900.00".into()),
            )])),
            "b1",
            None,
        )
        .unwrap();
    assert_eq!(
        r.get("leaf").and_then(Value::as_str),
        Some("manager_review")
    );
    let child = s
        .send_event("big", "withdraw", Value::Obj(BTreeMap::new()), "b2", None)
        .unwrap();
    assert_eq!(child.get("leaf").and_then(Value::as_str), Some("draft"));
    assert_eq!(
        child
            .get("transition")
            .and_then(|t| t.get("source_state"))
            .and_then(Value::as_str),
        Some("manager_review"),
        "the child-first override must beat review.withdraw"
    );

    s.create_instance("expense_approval", "neg", "c3", None)
        .unwrap();
    let err = s
        .send_event(
            "neg",
            "submit",
            Value::Obj(BTreeMap::from([(
                "amount".into(),
                Value::Str("-1.00".into()),
            )])),
            "n1",
            None,
        )
        .unwrap_err();
    assert_eq!(err.code, "run/invariant");
    assert_eq!(err.hint, "adjust the action or invariant nonneg");
    let ok = s
        .send_event(
            "neg",
            "submit",
            Value::Obj(BTreeMap::from([(
                "amount".into(),
                Value::Str("10.00".into()),
            )])),
            "n2",
            None,
        )
        .unwrap();
    assert_eq!(ok.get("leaf").and_then(Value::as_str), Some("peer_review"));
}

fn fsm() -> &'static str {
    env!("CARGO_BIN_EXE_fsm")
}

fn run_fsm(dir: &std::path::Path, args: &[&str], clock_ms: Option<&str>) -> (i32, String, String) {
    let out = std::process::Command::new(fsm())
        .args(args)
        .arg(format!("--data-dir={}", dir.display()))
        .env("FSM_CLOCK_MS", clock_ms.unwrap_or("5000"))
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(255),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn extract_fsm_line(line: &str) -> Option<(String, Option<String>)> {
    let t = line.trim();
    let (rest, clock_ms) = if let Some(rest) = t.strip_prefix("$ FSM_CLOCK_MS=") {
        let (clock_ms, command) = rest.split_once(" fsm ")?;
        (command, Some(clock_ms.to_string()))
    } else {
        (
            t.strip_prefix("$ fsm ")
                .or_else(|| t.strip_prefix("fsm "))?,
            None,
        )
    };
    if rest.starts_with("version") || rest.starts_with("docs ") || rest.starts_with("help") {
        return None;
    }
    Some((rest.to_string(), clock_ms))
}

fn split_cmd(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None::<char>;
    for c in rest.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c.is_whitespace() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

struct DocCmd {
    args: Vec<String>,
    clock_ms: Option<String>,
    expect_fail: bool,
    expect_code: Option<String>,
    expect_leaf: Option<String>,
    expect_ok: bool,
    expect_created: bool,
    expect_hint: Option<String>,
    expect_effects_pending: Option<String>,
}

fn parse_doc_commands(text: &str) -> Vec<Vec<DocCmd>> {
    let mut blocks: Vec<Vec<DocCmd>> = Vec::new();
    let mut cur: Vec<DocCmd> = Vec::new();
    let mut in_fence = false;
    let mut pending: Option<DocCmd> = None;
    let flush_pending = |cur: &mut Vec<DocCmd>, pending: &mut Option<DocCmd>| {
        if let Some(c) = pending.take() {
            cur.push(c);
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            if in_fence {
                flush_pending(&mut cur, &mut pending);
                if !cur.is_empty() {
                    blocks.push(std::mem::take(&mut cur));
                }
            }
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if let Some((rest, clock_ms)) = extract_fsm_line(t) {
                flush_pending(&mut cur, &mut pending);
                if !cur.is_empty() {
                    blocks.push(std::mem::take(&mut cur));
                }
                pending = Some(DocCmd {
                    args: split_cmd(&rest),
                    clock_ms,
                    expect_fail: false,
                    expect_code: None,
                    expect_leaf: None,
                    expect_ok: false,
                    expect_created: false,
                    expect_hint: None,
                    expect_effects_pending: None,
                });
            }
            continue;
        }
        if let Some((rest, clock_ms)) = extract_fsm_line(t) {
            flush_pending(&mut cur, &mut pending);
            pending = Some(DocCmd {
                args: split_cmd(&rest),
                clock_ms,
                expect_fail: false,
                expect_code: None,
                expect_leaf: None,
                expect_ok: false,
                expect_created: false,
                expect_hint: None,
                expect_effects_pending: None,
            });
            continue;
        }
        if let Some(cmd) = pending.as_mut() {
            if t == "# exit 1" {
                cmd.expect_fail = true;
            } else if t.starts_with("leaf:") {
                cmd.expect_leaf = Some(t.trim_start_matches("leaf:").trim().to_string());
            } else if t == "ok: true" {
                cmd.expect_ok = true;
            } else if t == "created: true" {
                cmd.expect_created = true;
            } else if let Some(v) = t.strip_prefix("hint:") {
                cmd.expect_hint = Some(v.trim().to_string());
            } else if let Some(v) = t.strip_prefix("effects_pending:") {
                cmd.expect_effects_pending = Some(v.trim().to_string());
            } else if cmd.expect_fail && t.contains('/') && !t.starts_with('#') && !t.is_empty() {
                cmd.expect_code = Some(t.to_string());
            }
        }
    }
    flush_pending(&mut cur, &mut pending);
    if !cur.is_empty() {
        blocks.push(cur);
    }
    blocks
}

fn rendered_field(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim_start()
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix(':'))
            .map(|value| value.trim().to_string())
    })
}

#[test]
fn readme_and_examples_commands_run() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut all_blocks = Vec::new();
    for name in ["README.md", "docs/EXAMPLES.md"] {
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let blocks = parse_doc_commands(&text);
        assert!(!blocks.is_empty(), "{name} extracted no fsm command blocks");
        all_blocks.extend(blocks);
    }
    let ncmd: usize = all_blocks.iter().map(|b| b.len()).sum();
    assert!(ncmd >= 20, "extracted {ncmd} documented commands");
    let mut saw_expense = false;
    let mut saw_order = false;
    let mut saw_invoice = false;
    let mut saw_order_ack_path = false;
    let mut saw_hint = false;
    let mut saw_pending_effect = false;
    let mut saw_pending_cleared = false;
    for block in all_blocks {
        let directory = TestDirectory::create();
        for cmd in block {
            let mut args: Vec<String> = Vec::new();
            for a in &cmd.args {
                if a.starts_with("examples/") {
                    args.push(root.join(a).to_string_lossy().into_owned());
                } else {
                    args.push(a.clone());
                }
            }
            if args.iter().any(|a| a.contains("expense_approval")) {
                saw_expense = true;
            }
            if args.iter().any(|a| a.contains("order_lifecycle")) {
                saw_order = true;
            }
            if args.iter().any(|a| a.contains("invoice_matching")) {
                saw_invoice = true;
            }
            if args.iter().any(|a| a == "ack") {
                saw_order_ack_path = true;
            }
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let (c, out, err) = run_fsm(directory.path(), &arg_refs, cmd.clock_ms.as_deref());
            if cmd.expect_fail {
                assert_ne!(c, 0, "expected failure for {args:?}\n{out}{err}");
                if let Some(code) = &cmd.expect_code {
                    assert!(
                        err.contains(code) || out.contains(code),
                        "missing {code} in {out}{err}"
                    );
                }
                let rendered = format!("{out}{err}");
                let expected_hint = cmd
                    .expect_hint
                    .as_ref()
                    .expect("documented rejection must show its exact hint");
                assert_eq!(
                    rendered_field(&rendered, "hint").as_deref(),
                    Some(expected_hint.as_str()),
                    "documented hint drifted for {args:?}: {rendered}"
                );
                saw_hint = true;
            } else {
                assert_eq!(c, 0, "cmd {args:?} failed\n{out}{err}");
            }
            if let Some(leaf) = &cmd.expect_leaf {
                assert!(
                    out.contains(leaf) || out.contains(&format!("leaf: {leaf}")),
                    "leaf {leaf} missing in {out}"
                );
            }
            if cmd.expect_ok {
                assert!(
                    (out.contains("ok") && out.contains("true"))
                        || (out.contains("created") && out.contains("true")),
                    "documented ok:true missing in {out}"
                );
            }
            if cmd.expect_created {
                assert!(out.contains("created") && out.contains("true"), "{out}");
            }
            if let Some(expected) = &cmd.expect_effects_pending {
                assert_eq!(
                    rendered_field(&out, "effects_pending").as_deref(),
                    Some(expected.as_str()),
                    "documented pending-effect output drifted for {args:?}: {out}"
                );
                saw_pending_effect |= !expected.is_empty();
                saw_pending_cleared |= expected.is_empty();
            }
        }
    }
    assert!(
        saw_expense && saw_order && saw_invoice,
        "missing example walkthrough"
    );
    assert!(saw_order_ack_path, "documented instance ack unused");
    assert!(saw_hint, "documented errors must render a hint");
    assert!(
        saw_pending_effect,
        "documented emitted effect was not checked"
    );
    assert!(
        saw_pending_cleared,
        "documented cleared outbox was not checked"
    );
}

#[test]
fn order_lifecycle_paths() {
    let directory = TestDirectory::create();
    let mut s = Store::open(directory.path()).unwrap();
    s.define_machine(load("order_lifecycle"), false, false)
        .unwrap();
    s.create_instance("order_lifecycle", "o1", "c1", None)
        .unwrap();
    s.send_event("o1", "place", Value::Obj(BTreeMap::new()), "p1", None)
        .unwrap();
    let pending = s.state.instances.get("o1").unwrap().pending.clone();
    assert!(!pending.is_empty());
    s.ack_effect("o1", &pending[0], "a1").unwrap();
    assert!(s.state.instances.get("o1").unwrap().pending.is_empty());
    let configuration = s.state.instances.get("o1").unwrap().configuration.clone();
    let pending_before_note = s.state.instances.get("o1").unwrap().pending.clone();
    s.send_event(
        "o1",
        "note_added",
        Value::Obj(BTreeMap::from([("text".into(), Value::Str("x".into()))])),
        "note",
        None,
    )
    .unwrap();
    assert_eq!(
        s.state.instances.get("o1").unwrap().configuration,
        configuration
    );
    assert_eq!(
        s.state.instances.get("o1").unwrap().pending,
        pending_before_note,
        "internal note_added must not re-run entry emits"
    );
    s.send_event("o1", "pick", Value::Obj(BTreeMap::new()), "p2", None)
        .unwrap();
    s.send_event("o1", "ship", Value::Obj(BTreeMap::new()), "p3", None)
        .unwrap();
    let mut stamp_clock = FixedClock::new(9_000, 1);
    let mut payload = Value::Obj(BTreeMap::new());
    s.send_event_stamp_on(
        &mut stamp_clock,
        "o1",
        "confirmed",
        &mut payload,
        "p4",
        None,
        &["at"],
    )
    .unwrap();
    assert_eq!(payload.get("at").and_then(Value::as_str), Some("9000"));
    assert_eq!(
        stamp_clock.now, 9_001,
        "the supplied clock was consumed once"
    );
    assert_eq!(s.records.last().unwrap().ts, 9_000);
    assert_eq!(
        s.state
            .instances
            .get("o1")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("closed")
    );

    s.create_instance("order_lifecycle", "o-noack", "c-noack", None)
        .unwrap();
    s.send_event("o-noack", "place", Value::Obj(BTreeMap::new()), "np", None)
        .unwrap();
    assert!(
        !s.state.instances.get("o-noack").unwrap().pending.is_empty(),
        "no-ack retains pending"
    );
    s.send_event("o-noack", "pick", Value::Obj(BTreeMap::new()), "np2", None)
        .unwrap();
    s.send_event("o-noack", "ship", Value::Obj(BTreeMap::new()), "np3", None)
        .unwrap();
    let mut payload = Value::Obj(BTreeMap::new());
    s.send_event_stamp_on(
        &mut stamp_clock,
        "o-noack",
        "confirmed",
        &mut payload,
        "np4",
        None,
        &["at"],
    )
    .unwrap();
    assert_eq!(
        s.state
            .instances
            .get("o-noack")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("closed")
    );
    assert!(
        !s.state.instances.get("o-noack").unwrap().pending.is_empty(),
        "terminal no-ack still retains pending"
    );

    s.create_instance("order_lifecycle", "o2", "c2", None)
        .unwrap();
    let err = s
        .send_event(
            "o2",
            "confirmed",
            Value::Obj(BTreeMap::from([("at".into(), Value::Str("1".into()))])),
            "bad",
            None,
        )
        .unwrap_err();
    assert_eq!(err.code, "run/unhandled");
    assert!(!err.hint.is_empty());
    let enabled = err
        .details
        .get("enabled_events")
        .and_then(Value::as_arr)
        .expect("enabled_events on order rejection");
    assert!(
        enabled
            .iter()
            .any(|e| e.get("event").and_then(Value::as_str) == Some("place")),
        "{enabled:?}"
    );
}

#[test]
fn invoice_matching_paths() {
    let directory = TestDirectory::create();
    let mut s = Store::open(directory.path()).unwrap();
    s.define_machine(load("invoice_matching"), false, false)
        .unwrap();
    s.create_instance("invoice_matching", "i1", "c1", None)
        .unwrap();
    s.send_event(
        "i1",
        "receive",
        Value::Obj(BTreeMap::from([(
            "amount".into(),
            Value::Str("40.00".into()),
        )])),
        "r1",
        None,
    )
    .unwrap();
    s.send_event(
        "i1",
        "receive",
        Value::Obj(BTreeMap::from([(
            "amount".into(),
            Value::Str("60.00".into()),
        )])),
        "r2",
        None,
    )
    .unwrap();
    let total = s
        .state
        .instances
        .get("i1")
        .unwrap()
        .ctx
        .get("received_total")
        .unwrap()
        .canonical_string();
    assert_eq!(total, "100.00");
    let ratio = s
        .state
        .instances
        .get("i1")
        .unwrap()
        .ctx
        .get("ratio")
        .unwrap()
        .canonical_string();
    assert_eq!(ratio, "1.0000");
    s.send_event("i1", "match", Value::Obj(BTreeMap::new()), "m1", None)
        .unwrap();
    assert_eq!(
        s.state
            .instances
            .get("i1")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("matched")
    );

    s.create_instance("invoice_matching", "i2", "c2", None)
        .unwrap();
    s.send_event(
        "i2",
        "receive",
        Value::Obj(BTreeMap::from([(
            "amount".into(),
            Value::Str("10.00".into()),
        )])),
        "x1",
        None,
    )
    .unwrap();
    let err = s
        .send_event("i2", "match", Value::Obj(BTreeMap::new()), "x2", None)
        .unwrap_err();
    assert_eq!(err.code, "run/not_enabled");
    assert!(!err.hint.is_empty());
    let hist = s.history_page("i2", 0, 20, true, true).unwrap();
    let entries = hist.get("entries").and_then(Value::as_arr).unwrap();
    let rejected = entries
        .iter()
        .find(|e| e.get("kind").and_then(Value::as_str) == Some("EventRejected"))
        .unwrap();
    assert!(rejected.get("trace").is_some(), "{rejected:?}");
    let tr = rejected.get("trace").unwrap();
    let rendered = fsm_core::canon::canon_bytes(tr);
    let bytes = String::from_utf8_lossy(&rendered);
    assert!(
        bytes.contains("90.00") && bytes.contains("0.50") && bytes.contains("100.00"),
        "invoice abs(...) guard bindings missing: {bytes}"
    );
}
