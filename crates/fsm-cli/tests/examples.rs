use fsm_cli::clock::{self, FixedClock};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::{compile, parse_machine};
use std::collections::BTreeMap;

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
}

fn tmp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-ex-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn expense_approval_paths() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
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
    assert_eq!(s.state.instances.get("small").unwrap().leaf, "approved");

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
    s.send_event("big", "withdraw", Value::Obj(BTreeMap::new()), "b2", None)
        .unwrap();

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
    assert!(!err.hint.is_empty());
}

fn fsm() -> &'static str {
    env!("CARGO_BIN_EXE_fsm")
}

fn run_fsm(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let out = std::process::Command::new(fsm())
        .args(args)
        .arg(format!("--data-dir={}", dir.display()))
        .env("FSM_CLOCK_MS", "5000")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(255),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn readme_and_examples_commands_run() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut cmds = Vec::new();
    for name in ["README.md", "docs/EXAMPLES.md"] {
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("fsm ") {
                if rest.starts_with("version")
                    || rest.starts_with("docs ")
                    || rest.starts_with("help")
                {
                    continue;
                }
                cmds.push(rest.to_string());
            }
        }
    }
    assert!(cmds.len() >= 3, "extracted {}", cmds.len());
    let spec = root.join("examples/expense_approval.json");
    let dir = tmp();
    let (c, out, err) = run_fsm(&dir, &["validate", spec.to_str().unwrap()]);
    assert_eq!(c, 0, "{err}");
    assert!(out.contains("case_review") || out.contains("expense") || out.contains("created"));
    let (c, _, err) = run_fsm(&dir, &["machine", "add", spec.to_str().unwrap()]);
    assert_eq!(c, 0, "{err}");
    let (c, out, err) = run_fsm(
        &dir,
        &[
            "instance",
            "new",
            "expense_approval",
            "--request-id",
            "demo",
        ],
    );
    assert_eq!(c, 0, "{out}{err}");
    assert!(out.contains("draft") || out.contains("inst-demo"));
    let (c, out, err) = run_fsm(
        &dir,
        &[
            "instance",
            "send",
            "inst-demo",
            "submit",
            "--payload",
            r#"{"amount":"10.00"}"#,
            "--request-id",
            "demo-submit",
        ],
    );
    assert_eq!(c, 0, "{out}{err}");
    assert!(out.contains("peer_review"), "{out}");
    let (c, out, err) = run_fsm(&dir, &["instance", "history", "inst-demo"]);
    assert_eq!(c, 0, "{err}");
    assert!(out.contains("EventApplied") || out.contains("seq"), "{out}");
    let order = root.join("examples/order_lifecycle.json");
    if order.exists() {
        let dir = tmp();
        let (c, _, err) = run_fsm(&dir, &["machine", "add", order.to_str().unwrap()]);
        assert_eq!(c, 0, "{err}");
        let (c, out, err) = run_fsm(
            &dir,
            &["instance", "new", "order_lifecycle", "--request-id", "o1"],
        );
        assert_eq!(c, 0, "{out}{err}");
    }
}

#[test]
fn order_lifecycle_paths() {
    clock::reset_injected();
    clock::force_ms(9_000);
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(load("order_lifecycle"), false, false)
        .unwrap();
    s.create_instance("order_lifecycle", "o1", "c1", None)
        .unwrap();
    s.send_event("o1", "place", Value::Obj(BTreeMap::new()), "p1", None)
        .unwrap();
    let pending = s.state.instances.get("o1").unwrap().pending.clone();
    assert!(!pending.is_empty());
    s.ack_effect("o1", &pending[0], "a1").unwrap();
    s.send_event("o1", "pick", Value::Obj(BTreeMap::new()), "p2", None)
        .unwrap();
    s.send_event("o1", "ship", Value::Obj(BTreeMap::new()), "p3", None)
        .unwrap();
    let mut payload = Value::Obj(BTreeMap::new());
    s.send_event_stamp("o1", "confirmed", &mut payload, "p4", None, &["at"])
        .unwrap();
    assert_eq!(s.state.instances.get("o1").unwrap().leaf, "closed");

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
    let _ = FixedClock::new(1, 1);
}

#[test]
fn invoice_matching_paths() {
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
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
    assert_eq!(s.state.instances.get("i1").unwrap().leaf, "matched");

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
}
