//! Every audit tool, against a healthy store and three broken ones.
//!
//! These tools are only worth having if they are right about a **broken**
//! store, so the broken ones are built here rather than committed: a corrupt
//! binary fixture is a fixture nobody can review.
//!
//! Plan 0014 task 6802.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;

use fsm_cli::clock::FixedClock;
use fsm_cli::journal_io::{JournalHealth, classify};
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::{ServeMode, serve_dir_with};
use fsm_cli::mcp::tools::{DEGRADED_TOOLS, ToolCtx, dispatch_degraded};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

const SPEC: &str = include_str!("../../../docs/SPEC.md");

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
        let _ = fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The injected clock is process-global, so the tests that pin it take turns.
static CLOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-auditg-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"audit_case","context":[{"name":"seen","ty":"int","init":"0"}],"states":[{"name":"open"},{"name":"held"}],"initial":"open","events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held","do":[{"target":"seen","value":"ctx.seen + 1"}]},{"from":"held","on":"push","to":"open"}]}"#;

/// A store with a machine, an instance, and two applied events.
fn healthy(dir: &Scratch) {
    // Genesis and lock stamps are not clock-injected, so the process clock is
    // pinned for the open the way `mcp_full` pins it — otherwise two runs
    // journal different timestamps and hash differently.
    fsm_cli::clock::reset_injected();
    fsm_cli::clock::force_ms(1_000);
    fsm_cli::clock::set_step(0);
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 0);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "audit_case",
            "inst-a",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    for n in 0..2 {
        let mut payload = Value::Obj(BTreeMap::new());
        store
            .send_event_stamp_on(
                &mut clock,
                "inst-a",
                "push",
                &mut payload,
                &format!("push-{n}"),
                None,
                &[],
            )
            .unwrap();
    }
    drop(store);
    fsm_cli::clock::reset_injected();
}

fn segment(dir: &Scratch) -> std::path::PathBuf {
    dir.join("journal/seg-00000000000000000000.jsonl")
}

/// The three ways a journal breaks, built here rather than committed.
fn damaged(tag: &str, kind: &str) -> Scratch {
    let dir = scratch(tag);
    healthy(&dir);
    let path = segment(&dir);
    match kind {
        // A space inside a record: parseable JSON, not the canonical bytes
        // the hash covers.
        "non_canonical" => {
            let mut bytes = fs::read(&path).unwrap();
            let position = bytes.iter().position(|b| *b == b'{').unwrap();
            bytes.insert(position + 1, b' ');
            fs::write(&path, &bytes).unwrap();
        }
        // A final record cut off mid-line.
        "torn" => {
            let mut bytes = fs::read(&path).unwrap();
            bytes.truncate(bytes.len() - 4);
            fs::write(&path, &bytes).unwrap();
        }
        // A `prev` that no longer names the record before it.
        "chain" => {
            let text = fs::read_to_string(&path).unwrap();
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            let target = lines.len() - 1;
            let at = lines[target].find("\"prev\":\"").unwrap() + "\"prev\":\"".len();
            lines[target].replace_range(at..at + 64, &"b".repeat(64));
            fs::write(&path, lines.join("\n") + "\n").unwrap();
        }
        other => panic!("no damage authored for {other}"),
    }
    dir
}

fn field(report: &Value, name: &str) -> Option<String> {
    report.get(name).and_then(|v| match v {
        Value::Str(s) => Some(s.clone()),
        Value::Num(n) => Some(n.clone()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn call(dir: &Scratch, name: &str, args: &str) -> Result<Value, fsm_cli::store::ErrorObj> {
    dispatch_degraded(
        dir,
        &mut FixedClock::new(2_000, 1),
        name,
        &value(args),
        &ToolCtx::default(),
    )
}

/// One session over a healthy store, exercising every audit tool.
fn audit_session(dir: &Scratch) -> String {
    let sink = SharedSink::new();
    let lines = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"store_doctor","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"journal_verify","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"journal_replay","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"explain_step","arguments":{"instance_id":"inst-a","seq":3}}}"#,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"instance_annotate","arguments":{"instance_id":"inst-a","note":"checked by the audit suite","request_id":"audit-note"}}}"#,
    ];
    let input: String = lines.iter().map(|line| format!("{line}\n")).collect();
    serve_dir_with(
        dir,
        ServeMode::Writer,
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    sink.text()
}

fn fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audit/session.expected")
}

#[test]
fn one_healthy_session_over_every_audit_tool() {
    let _turn = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("session");
    healthy(&dir);
    let stream = audit_session(&dir);

    // Hand-derived: five calls, five answers, in order, and nothing else.
    let shape: Vec<String> = stream
        .lines()
        .map(|line| {
            let message = parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("a JSON line");
            match message.get("method").and_then(Value::as_str) {
                Some(method) => method.to_string(),
                None => format!(
                    "reply:{}",
                    message.get("id").and_then(Value::as_num).unwrap_or("?")
                ),
            }
        })
        .collect();
    assert_eq!(
        shape,
        [
            "reply:1", "reply:2", "reply:3", "reply:4", "reply:5", "reply:6"
        ]
    );

    if std::env::var("REGEN_AUDIT").ok().as_deref() == Some("1") {
        fs::write(fixture(), &stream).unwrap();
        return;
    }
    assert_eq!(stream, fs::read_to_string(fixture()).unwrap_or_default());
}

#[test]
fn the_fixture_carries_nothing_about_the_machine_that_made_it() {
    let text = fs::read_to_string(fixture()).unwrap_or_default();
    assert!(!text.is_empty(), "fixture missing");
    assert!(!text.contains("/tmp"), "an absolute path leaked");
    assert!(!text.contains("fsm-auditg-"), "a temp directory leaked");
    assert!(!text.contains('\r'), "a line ending leaked");
    assert!(
        !text.contains(&format!("\"holder\":{}", std::process::id())),
        "a pid leaked"
    );
}

#[test]
fn every_damaged_store_reports_the_health_spec_names() {
    for (tag, expected) in [
        ("non_canonical", "NonCanonical"),
        ("torn", "TornTail"),
        ("chain", "ChainBroken"),
    ] {
        let dir = damaged(tag, tag);
        let health = classify(&dir);
        // The classifier is the authority; the tools must agree with it and
        // with SPEC's vocabulary.
        let named = match &health {
            JournalHealth::NonCanonical { .. } => "NonCanonical",
            JournalHealth::TornTail { .. } => "TornTail",
            JournalHealth::ChainBroken { .. } => "ChainBroken",
            other => panic!("{tag}: unexpected health {other:?}"),
        };
        assert_eq!(named, expected, "{tag}");

        let doctor = call(&dir, "store_doctor", "{}").unwrap();
        assert_eq!(field(&doctor, "health").as_deref(), Some(expected), "{tag}");
        let verified = call(&dir, "journal_verify", "{}").unwrap();
        assert_eq!(
            field(&verified, "health").as_deref(),
            Some(expected),
            "{tag}: journal_verify and store_doctor must not disagree"
        );

        // And SPEC's posture: a remedy where the table gives one, none where
        // it says there is no repair.
        match expected {
            "TornTail" => {
                let remedy = field(&doctor, "remedy").expect("the table gives one");
                assert_eq!(remedy, "fsm repair --truncate-torn-tail");
                assert!(SPEC.contains(&remedy), "not SPEC's words: {remedy}");
                assert_eq!(field(&verified, "remedy"), Some(remedy));
            }
            _ => {
                assert!(
                    doctor.get("remedy").is_none() && verified.get("remedy").is_none(),
                    "{tag}: interior damage has no repair, and offering one would be a lie"
                );
            }
        }
    }
}

#[test]
fn a_degraded_server_serves_the_diagnosis_and_refuses_the_rest() {
    for tag in ["non_canonical", "chain"] {
        let dir = damaged(tag, tag);
        assert!(Store::open(&dir).is_err(), "{tag} still opens");

        // The three that answer.
        for name in DEGRADED_TOOLS {
            call(&dir, name, "{}").unwrap_or_else(|e| panic!("{tag}/{name}: {e:?}"));
        }
        // One that does not, refused with the same facts.
        let doctor = call(&dir, "store_doctor", "{}").unwrap();
        let refused = call(&dir, "instance_get", r#"{"instance_id":"inst-a"}"#)
            .expect_err("a read needs a store");
        assert_eq!(refused.code, "store/degraded");
        assert_eq!(refused.details.get("health"), doctor.get("health"), "{tag}");
        assert_eq!(refused.details.get("remedy"), doctor.get("remedy"), "{tag}");

        // And authoring still works, because it needs no store.
        let dry = format!(r#"{{"dry_run":true,"spec":{CASE}}}"#);
        call(&dir, "machine_create", &dry)
            .unwrap_or_else(|e| panic!("{tag}: a dry run needs no store: {e:?}"));
    }
}

#[test]
fn the_tools_and_the_command_line_agree_on_every_fixture() {
    // Divergence between the two surfaces is the failure most likely to go
    // unnoticed, so it is asserted for each pair on each fixture.
    let healthy_dir = scratch("parity");
    healthy(&healthy_dir);
    let store = Store::open_read_only(&healthy_dir).unwrap();
    let explained_by_store = store.explain_seq("inst-a", 3).unwrap();
    drop(store);
    let explained_by_tool = call(
        &healthy_dir,
        "explain_step",
        r#"{"instance_id":"inst-a","seq":3}"#,
    );
    // A healthy store dispatches through the ordinary path; the degraded
    // dispatcher refuses it, which is itself the agreement being asserted.
    assert!(
        explained_by_tool.is_err(),
        "a healthy store is not degraded"
    );
    let mut store = Store::open(&healthy_dir).unwrap();
    let explained_by_tool = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut FixedClock::new(2_000, 1),
        "explain_step",
        &value(r#"{"instance_id":"inst-a","seq":3}"#),
    )
    .unwrap();
    assert_eq!(
        fsm_core::canon::canon_bytes(&explained_by_tool),
        fsm_core::canon::canon_bytes(&explained_by_store),
        "explain_step and fsm explain --json must be the same answer"
    );
    drop(store);

    for tag in ["non_canonical", "torn", "chain"] {
        let dir = damaged(tag, tag);
        let health = classify(&dir);
        let doctor = call(&dir, "store_doctor", "{}").unwrap();
        assert!(
            format!("{health:?}").starts_with(&field(&doctor, "health").unwrap()),
            "{tag}: store_doctor and fsm doctor read the same classification"
        );
        let verified = call(&dir, "journal_verify", "{}").unwrap();
        assert_eq!(
            field(&verified, "message"),
            Some(health.message()),
            "{tag}: journal_verify and fsm journal verify say the same sentence"
        );
    }
}

#[test]
fn no_read_side_tool_touches_the_store() {
    let dir = scratch("readonly");
    healthy(&dir);
    let before = fs::read(segment(&dir)).unwrap();
    let version_before = fs::read(dir.join("VERSION")).unwrap_or_default();
    let snapshots_before = fsm_cli::snapshot::listed_snaps(&dir).len();

    // Read-only, because dropping a *writable* handle writes a shutdown
    // snapshot — which would be the store closing, not a tool writing.
    let mut store = Store::open_read_only(&dir).unwrap();
    for (name, args) in [
        ("store_doctor", "{}"),
        ("journal_verify", "{}"),
        ("journal_replay", "{}"),
        ("explain_step", r#"{"instance_id":"inst-a","seq":3}"#),
        ("instance_get", r#"{"instance_id":"inst-a"}"#),
    ] {
        fsm_cli::mcp::tools::dispatch(
            &mut store,
            &mut FixedClock::new(2_000, 1),
            name,
            &value(args),
        )
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    drop(store);

    assert_eq!(
        fs::read(segment(&dir)).unwrap(),
        before,
        "the journal moved"
    );
    assert_eq!(
        fs::read(dir.join("VERSION")).unwrap_or_default(),
        version_before,
        "VERSION was rewritten"
    );
    assert_eq!(
        fsm_cli::snapshot::listed_snaps(&dir).len(),
        snapshots_before,
        "a read created or removed a snapshot"
    );
}

#[test]
fn verify_and_replay_are_both_needed() {
    // The pair that justifies having two tools: bytes clean, semantics not.
    let dir = scratch("divergent");
    healthy(&dir);
    let path = segment(&dir);
    let text = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut tampered = None;
    for line in lines.iter_mut() {
        let Some(at) = line.find("\"state_hash\":\"sha256:") else {
            continue;
        };
        let start = at + "\"state_hash\":\"sha256:".len();
        line.replace_range(start..start + 64, &"c".repeat(64));
        tampered = Some(
            parse(line.as_bytes(), &JsonLimits::DEFAULT)
                .unwrap()
                .get("seq")
                .and_then(Value::as_num)
                .unwrap()
                .parse::<u64>()
                .unwrap(),
        );
        break;
    }
    let tampered = tampered.expect("a record carries a state hash");
    // Re-seal the chain over the change, so the bytes are exactly as they
    // should be and only the recorded claim is wrong.
    let mut prev = fsm_core::record::zeros();
    let mut out = Vec::new();
    for line in &lines {
        let parsed = parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        let get = |name: &str| parsed.get(name).cloned().unwrap_or(Value::Null);
        let sealed = fsm_core::record::seal(
            get("seq").as_num().unwrap().parse().unwrap(),
            get("ts").as_num().unwrap().parse().unwrap(),
            fsm_core::record::RecordKind::from_str(get("kind").as_str().unwrap()).unwrap(),
            get("body"),
            &prev,
        );
        prev = sealed.hash.clone();
        out.push(String::from_utf8(fsm_core::canon::canon_bytes(&sealed.to_value())).unwrap());
    }
    fs::write(&path, out.join("\n") + "\n").unwrap();

    let verified = call(&dir, "journal_verify", "{}").unwrap();
    assert_eq!(
        field(&verified, "health").as_deref(),
        Some("Ok"),
        "the bytes and the chain are exactly as they should be"
    );
    let replayed = call(&dir, "journal_replay", "{}").unwrap();
    assert_eq!(
        field(&replayed, "matches").as_deref(),
        Some("false"),
        "and the engine still disagrees with what was recorded"
    );
    assert_eq!(
        field(&replayed, "first_divergence_seq").as_deref(),
        Some(tampered.to_string().as_str())
    );
}
