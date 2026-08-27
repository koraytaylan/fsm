//! Every affordance this plan adds, as one byte-compared session.
//!
//! Four of them are wire shapes a client parses — annotations, titles,
//! completions, and the elicitation exchange — so the proof is the whole
//! stream, in order, with nothing extra. The interleaving the design
//! permits is in it too: a client request answered *while* an elicitation
//! is outstanding, which is the case most likely to break in a refactor and
//! least likely to be noticed.
//!
//! Plan 0013 task 6501.

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;

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

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The server's own request ids are process-wide, and this suite writes them
/// into its script, so its sessions take turns.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-afford-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"afford_case","states":[{"name":"waiting"},{"name":"held"},{"name":"done","terminal":true}],"initial":"waiting","context":[],"events":[{"name":"decide","fields":[{"name":"verdict","ty":"str"}]},{"name":"defer","fields":[]}],"transitions":[{"from":"waiting","on":"decide","to":"held"},{"from":"waiting","on":"defer","to":"done"},{"from":"held","on":"defer","to":"done"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 0);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "afford_case",
            "inst-afford",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

/// The session: every list shape, four completions, both elicitation
/// outcomes, and one client request answered mid-elicitation.
fn script(machine_id: &str) -> String {
    [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"elicitation":{}},"clientInfo":{"name":"golden","version":"1"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":4,"method":"resources/templates/list"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":5,"method":"prompts/list"}"#.to_string(),
        // A machine id, by the first two characters of the one that exists.
        format!(
            r#"{{"jsonrpc":"2.0","id":6,"method":"completion/complete","params":{{"ref":{{"type":"ref/resource","uri":"fsm://machine/{{id}}"}},"argument":{{"name":"id","value":"{}"}}}}}}"#,
            &machine_id[..2]
        ),
        r#"{"jsonrpc":"2.0","id":7,"method":"completion/complete","params":{"ref":{"type":"ref/resource","uri":"fsm://instance/{id}"},"argument":{"name":"id","value":"inst-"}}}"#.to_string(),
        // An event, completed from the instance the client already named …
        r#"{"jsonrpc":"2.0","id":8,"method":"completion/complete","params":{"ref":{"type":"ref/prompt","name":"drive_instance"},"argument":{"name":"event","value":""},"context":{"arguments":{"instance_id":"inst-afford"}}}}"#.to_string(),
        // … and the same question without it, which is answered emptily
        // rather than guessed at.
        r#"{"jsonrpc":"2.0","id":9,"method":"completion/complete","params":{"ref":{"type":"ref/prompt","name":"drive_instance"},"argument":{"name":"event","value":""}}}"#.to_string(),
        // An ask the person accepts. The server writes `fsm-elicit-1` and
        // then reads: a client request first, answered while the question is
        // outstanding, and the answer after it.
        r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"instance_elicit","arguments":{"instance_id":"inst-afford","event":"decide","request_id":"afford-1","message":"Decide this case"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":11,"method":"ping"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":"fsm-elicit-1","result":{"action":"accept","content":{"verdict":"approve"}}}"#.to_string(),
        // And one the person declines.
        r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"instance_elicit","arguments":{"instance_id":"inst-afford","event":"defer","request_id":"afford-2"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":"fsm-elicit-2","result":{"action":"decline"}}"#.to_string(),
    ]
    .join("\n")
        + "\n"
}

fn drive(dir: &Scratch) -> (String, Scratch) {
    let mut store = seeded(dir);
    let machine_id = store
        .state
        .machines
        .keys()
        .next()
        .cloned()
        .expect("one machine");
    fsm_cli::mcp::elicit::reset_request_ids();
    let sink = SharedSink::new();
    serve_session(
        Some(&mut store),
        &mut FixedClock::new(1_000, 0),
        std::io::Cursor::new(script(&machine_id).into_bytes()),
        sink.writer(),
    )
    .unwrap();
    drop(store);
    (sink.text(), Scratch(dir.to_path_buf()))
}

/// Each written line as one word: a reply to id N, or a message by method.
fn shape(stream: &str) -> Vec<String> {
    stream
        .lines()
        .map(|line| {
            let message = parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("a JSON line");
            match message.get("method").and_then(Value::as_str) {
                Some(method) => method.to_string(),
                None => format!(
                    "reply:{}",
                    message
                        .get("id")
                        .and_then(|id| id.as_num().or_else(|| id.as_str()))
                        .unwrap_or("?")
                ),
            }
        })
        .collect()
}

fn fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_affordance/session.expected")
}

#[test]
fn one_session_covers_every_affordance_byte_for_byte() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("golden");
    let (stream, _keep) = drive(&dir);

    // Hand-derived: what the specification and this plan say the session
    // produces, in order. The two elicitation requests are messages the
    // *server* sends, and the ping between the first one and its answer is
    // the interleaving the design permits.
    assert_eq!(
        shape(&stream),
        vec![
            "reply:1",            // initialize
            "reply:2",            // tools/list
            "reply:3",            // resources/list
            "reply:4",            // resources/templates/list
            "reply:5",            // prompts/list
            "reply:6",            // completion: machine id
            "reply:7",            // completion: instance id
            "reply:8",            // completion: event, with context
            "reply:9",            // completion: event, without it
            "elicitation/create", // the ask …
            "reply:11",           // … a client request answered mid-ask …
            "reply:10",           // … then the tool result
            "elicitation/create", // the second ask
            "reply:12",           // declined
        ],
        "the stream was:\n{stream}"
    );

    if std::env::var("REGEN_AFFORDANCE").ok().as_deref() == Some("1") {
        std::fs::write(fixture(), &stream).unwrap();
        return;
    }
    assert_eq!(
        stream,
        std::fs::read_to_string(fixture()).unwrap_or_default()
    );
}

#[test]
fn the_same_session_twice_writes_the_same_bytes() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let first = {
        let dir = scratch("twice-a");
        drive(&dir).0
    };
    let second = {
        let dir = scratch("twice-b");
        drive(&dir).0
    };
    assert_eq!(first, second);
}

#[test]
fn the_accepted_ask_sends_and_the_declined_one_does_not() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("journal");
    let (_, _keep) = drive(&dir);
    let store = Store::open_read_only(&dir).unwrap();
    let applied: Vec<&Value> = store
        .records
        .iter()
        .filter(|r| r.kind == RecordKind::EventApplied)
        .map(|r| &r.body)
        .collect();
    assert_eq!(
        applied.len(),
        1,
        "the accepted ask sent one event and the declined one sent none"
    );
    assert_eq!(
        applied[0].get("event").and_then(Value::as_str),
        Some("decide")
    );
    assert!(
        store
            .records
            .iter()
            .all(|r| r.kind != RecordKind::RequestRejected),
        "a decline is not a refusal to record"
    );
}

#[test]
fn the_answers_are_the_ones_this_plan_promised() {
    // The fixture is the byte comparison; these are the claims inside it
    // that a reader of the diff would otherwise have to verify by eye.
    let text = std::fs::read_to_string(fixture()).unwrap_or_default();
    assert!(!text.is_empty(), "fixture missing");
    let messages: Vec<Value> = text
        .lines()
        .map(|l| parse(l.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .collect();
    let reply = |id: &str| -> Value {
        messages
            .iter()
            .find(|m| m.get("id").and_then(|i| i.as_num().or_else(|| i.as_str())) == Some(id))
            .expect("a reply")
            .get("result")
            .expect("a result")
            .clone()
    };

    // Every tool carries a title and four hints, and `instance_cancel` is
    // the only destructive one.
    let tools = reply("2");
    let listed = tools.get("tools").and_then(Value::as_arr).unwrap();
    assert_eq!(listed.len(), 20);
    for tool in listed {
        let name = tool.get("name").and_then(Value::as_str).unwrap();
        assert!(tool.get("title").is_some(), "{name} has no title");
        let annotations = tool.get("annotations").expect("annotations");
        for hint in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert!(annotations.get(hint).is_some(), "{name} lacks {hint}");
        }
    }

    // Completions: an event completed from the named instance, and the same
    // question without the context answered emptily rather than guessed.
    let with_context = reply("8");
    let values: Vec<&str> = with_context
        .get("completion")
        .and_then(|c| c.get("values"))
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(values, ["decide", "defer"]);
    let without = reply("9");
    assert_eq!(
        without
            .get("completion")
            .and_then(|c| c.get("values"))
            .and_then(Value::as_arr)
            .map(|values| values.len()),
        Some(0),
        "an empty answer is present in the stream, not absent from it"
    );

    // The declined ask says which way it went and that nothing was applied.
    let declined = reply("12");
    let structured = declined.get("structuredContent").unwrap_or(&declined);
    assert_eq!(
        structured.get("action").and_then(Value::as_str),
        Some("decline")
    );
    assert_eq!(
        structured.get("applied").and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn the_fixture_carries_nothing_about_the_machine_that_made_it() {
    let text = std::fs::read_to_string(fixture()).unwrap_or_default();
    assert!(!text.is_empty(), "fixture missing");
    assert!(!text.contains("/tmp"), "an absolute path leaked");
    assert!(!text.contains("fsm-afford-"), "a temp directory leaked");
    assert!(!text.contains('\r'), "a line ending leaked");
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'T' || index == 0 || index + 3 >= bytes.len() {
            continue;
        }
        assert!(
            !(bytes[index - 1].is_ascii_digit()
                && bytes[index + 1].is_ascii_digit()
                && bytes[index + 2].is_ascii_digit()
                && bytes[index + 3] == b':'),
            "a timestamp leaked at byte {index}"
        );
    }
}
