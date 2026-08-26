//! Who may write while the executor runs. Three modes, each pinned by what it
//! refuses: read-only refuses every mutator, embedded refuses nothing but only
//! ticks when the client speaks, and exclusive refuses to start beside another
//! writer.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::SystemClock;
use fsm_cli::mcp::serve::{ExecutorLoop, serve_session, serve_session_with};
use fsm_cli::mcp::tools::MUTATING_TOOLS;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_execute::config::HandlerTable;
use fsm_store::clock::FixedClock;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_fsm")
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-serve-modes-{test_name}-{}-{sequence}",
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

/// The stub handler: this test binary re-executed with a marker the harness
/// ignores.
#[test]
fn stub_handler() {
    if std::env::args().any(|argument| argument == "stub:ok") {
        std::process::exit(0);
    }
}

fn machine_source() -> &'static str {
    r#"{
        "format":"fsm.machine/1",
        "name":"order_confirmation_serve",
        "context":[{"name":"order_id","ty":"str","init":"order-7"}],
        "events":[
            {"name":"submit","fields":[]},
            {"name":"confirmed","fields":[{"name":"at","ty":"timestamp"}]},
            {"name":"confirmation_failed","fields":[]}
        ],
        "effects":[{"name":"request_confirmation","fields":[{"name":"order","ty":"str"}]}],
        "states":[
            {"name":"placed"},
            {"name":"awaiting_confirmation","entry":{"emit":[
                {"effect":"request_confirmation","args":{"order":"ctx.order_id"}}
            ]}},
            {"name":"confirmed_order","terminal":true},
            {"name":"unconfirmed","terminal":true}
        ],
        "initial":"placed",
        "transitions":[
            {"from":"placed","on":"submit","to":"awaiting_confirmation"},
            {"from":"awaiting_confirmation","on":"confirmed","to":"confirmed_order"},
            {"from":"awaiting_confirmation","on":"confirmation_failed","to":"unconfirmed"}
        ]
    }"#
}

fn machine() -> Value {
    parse(machine_source().as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn stub_table_json() -> String {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        .into_owned();
    format!(
        r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"request_confirmation","argv":["{stub}","stub_handler","--exact","--nocapture","stub:ok"],"timeout_ms":30000,"on_ok":{{"event":"confirmed","payload":{{}},"stamps":["at"]}},"on_failed":{{"event":"confirmation_failed"}}}}]}}"#
    )
}

/// Open a writer, tolerating a lock this process itself just released.
///
/// Several tests here spawn child processes, and spawning forks: between
/// `fork` and `exec` the child holds a copy of every open descriptor, so an
/// advisory lock dropped a moment ago can still be held for the length of that
/// window. That is a real property of a process-spawning executor — the
/// executor's own loop retries for the same reason — and not something a test
/// should assert away by being lucky.
fn open_writer(path: &Path) -> Store {
    for _ in 0..50 {
        match Store::open(path) {
            Ok(store) => return store,
            Err(error) if error.code == "store/lock" => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("open writer {}: {error:?}", path.display()),
        }
    }
    panic!("the writer lock on {} never became free", path.display())
}

/// Define the machine and create one instance, then release the writer.
fn seeded(directory: &TestDirectory) {
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation_serve",
            "order-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
}

fn initialize_lines() -> String {
    concat!(
        r#"{"id":1,"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{},"clientInfo":{"name":"test","version":"0"},"protocolVersion":"2025-06-18"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n"
    )
    .to_string()
}

fn call(id: u32, name: &str, arguments: &str) -> String {
    format!(
        r#"{{"id":{id},"jsonrpc":"2.0","method":"tools/call","params":{{"name":"{name}","arguments":{arguments}}}}}"#
    ) + "\n"
}

fn responses(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("one JSON object per line"))
        .collect()
}

fn tool_text(response: &Value) -> String {
    response
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_arr)
        .and_then(|content| content.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_error(response: &Value) -> bool {
    response
        .get("result")
        .and_then(|result| result.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The machine definition as one canonical line, so it can be embedded in a
/// newline-delimited request.
fn machine_line() -> String {
    String::from_utf8(fsm_core::canon::canon_bytes(&machine())).expect("canonical JSON is UTF-8")
}

/// Arguments that satisfy each mutating tool's schema, so the refusal under
/// test is the read-only gate and not a shape error.
fn arguments_for(tool: &str) -> String {
    match tool {
        "machine_create" => format!(r#"{{"spec":{}}}"#, machine_line()),
        "instance_create" => {
            r#"{"machine":"order_confirmation_serve","request_id":"req-a"}"#.to_string()
        }
        "instance_send" => {
            r#"{"instance_id":"order-1","event":{"name":"submit"},"request_id":"req-b"}"#
                .to_string()
        }
        "deadline_poll" => r#"{"instance_id":"order-1","request_id":"req-c"}"#.to_string(),
        "effect_ack" => {
            r#"{"instance_id":"order-1","effect_id":"order-1/3/0","request_id":"req-d"}"#
                .to_string()
        }
        "instance_cancel" => {
            r#"{"instance_id":"order-1","reason":"stopped","request_id":"req-e"}"#.to_string()
        }
        "invocation_start" | "invocation_return" => {
            r#"{"instance_id":"order-1","slot":"review","request_id":"req-f"}"#.to_string()
        }
        "signal_deliver" => {
            r#"{"instance_id":"order-1","signal_id":"order-1/3/0","request_id":"req-g"}"#
                .to_string()
        }
        other => panic!("no arguments authored for {other}"),
    }
}

#[test]
fn a_read_only_server_refuses_every_mutating_tool_by_name() {
    let directory = TestDirectory::create("read-only-refusals");
    seeded(&directory);

    let mut input = initialize_lines();
    for (index, tool) in MUTATING_TOOLS.iter().enumerate() {
        input.push_str(&call(100 + index as u32, tool, &arguments_for(tool)));
    }
    let mut store = Store::open_read_only(directory.path()).unwrap();
    let mut clock = SystemClock;
    let mut output = Vec::new();
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(input.as_bytes()),
        &mut output,
    )
    .unwrap();

    let replies = responses(&String::from_utf8(output).unwrap());
    let refusals: Vec<&Value> = replies.iter().skip(1).collect();
    assert_eq!(refusals.len(), MUTATING_TOOLS.len());
    for (tool, reply) in MUTATING_TOOLS.iter().zip(refusals) {
        assert!(is_error(reply), "{tool} was not refused: {reply:?}");
        let text = tool_text(reply);
        assert!(text.contains("read-only"), "{tool}: {text}");
        assert!(text.contains("executor"), "{tool}: {text}");
        assert!(text.contains("io/write"), "{tool}: {text}");
    }
}

#[test]
fn a_read_only_server_still_validates_a_dry_run_definition() {
    let directory = TestDirectory::create("read-only-dry-run");
    seeded(&directory);
    let input = initialize_lines()
        + &call(
            2,
            "machine_create",
            &format!(r#"{{"spec":{},"dry_run":true}}"#, machine_line()),
        );

    let mut store = Store::open_read_only(directory.path()).unwrap();
    let mut clock = SystemClock;
    let mut output = Vec::new();
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(input.as_bytes()),
        &mut output,
    )
    .unwrap();

    let replies = responses(&String::from_utf8(output).unwrap());
    assert!(
        !is_error(&replies[1]),
        "a dry run validates without writing: {:?}",
        replies[1]
    );
}

#[test]
fn a_read_only_server_sees_writes_that_land_after_it_started() {
    // The whole point of the mode is watching the executor work. A handle
    // opened once is one frozen prefix, so the session reopens per request.
    let directory = TestDirectory::create("read-only-fresh");
    seeded(&directory);

    let mut store = Store::open_read_only(directory.path()).unwrap();
    let mut clock = SystemClock;
    let mut output = Vec::new();
    serve_session_with(
        Some(&mut store),
        &mut clock,
        None,
        Some(directory.path()),
        Cursor::new(initialize_lines().as_bytes()),
        &mut output,
    )
    .unwrap();

    // Something else advances the instance while the session is up.
    let mut writer = open_writer(directory.path());
    let mut writer_clock = FixedClock::new(9_000, 1);
    writer
        .send_event_stamp_on(
            &mut writer_clock,
            "order-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    drop(writer);

    // A second session over the same handle: the reopen is what decides what
    // it can see, not the handle it was handed.
    let later = initialize_lines() + &call(2, "instance_get", r#"{"instance_id":"order-1"}"#);
    let mut output = Vec::new();
    serve_session_with(
        Some(&mut store),
        &mut clock,
        None,
        Some(directory.path()),
        Cursor::new(later.as_bytes()),
        &mut output,
    )
    .unwrap();
    let text = tool_text(&responses(&String::from_utf8(output).unwrap())[1]);
    assert!(
        text.contains("awaiting_confirmation"),
        "a monitoring session must see the new prefix: {text}"
    );
    assert!(
        text.contains("order-1/3/0"),
        "including the effect the executor is about to run: {text}"
    );
}

#[test]
fn a_read_only_server_reads_a_store_a_writer_is_changing() {
    let directory = TestDirectory::create("read-only-coexists");
    seeded(&directory);

    // The writer stays open for the whole session: this is paired mode.
    let mut writer = open_writer(directory.path());
    let mut store = Store::open_read_only(directory.path()).expect("read-only takes no lock");
    let input = initialize_lines()
        + &call(2, "instance_get", r#"{"instance_id":"order-1"}"#)
        + &call(3, "instance_history", r#"{"instance_id":"order-1"}"#);
    let mut clock = SystemClock;
    let mut output = Vec::new();
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(input.as_bytes()),
        &mut output,
    )
    .unwrap();

    let replies = responses(&String::from_utf8(output).unwrap());
    assert!(!is_error(&replies[1]), "{:?}", replies[1]);
    assert!(!is_error(&replies[2]), "{:?}", replies[2]);
    let mut clock = FixedClock::new(9_000, 1);
    writer
        .send_event_stamp_on(
            &mut clock,
            "order-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .expect("the writer never lost its lock");
}

#[test]
fn a_second_writer_is_still_refused_within_one_process() {
    let directory = TestDirectory::create("writer-contention");
    seeded(&directory);
    let held = open_writer(directory.path());
    match Store::open(directory.path()) {
        Ok(_) => panic!("a second writer must not open the same data dir"),
        Err(error) => assert_eq!(error.code, "store/lock"),
    }
    drop(held);
}

#[test]
fn embedded_mode_journals_the_ack_but_only_when_the_client_speaks() {
    let directory = TestDirectory::create("embedded");
    seeded(&directory);
    let table = HandlerTable::parse(&stub_table_json()).unwrap();
    let mut executor = ExecutorLoop::new(directory.path(), table).unwrap();
    let mut store = open_writer(directory.path());
    let mut clock = SystemClock;

    // One line: advance into the state that emits. The tick that follows it
    // observes the effect and spawns the handler.
    let mut output = Vec::new();
    let input = initialize_lines()
        + &call(
            2,
            "instance_send",
            r#"{"instance_id":"order-1","event":{"name":"submit"},"request_id":"req-submit"}"#,
        );
    serve_session_with(
        Some(&mut store),
        &mut clock,
        Some(&mut executor),
        None,
        Cursor::new(input.as_bytes()),
        &mut output,
    )
    .unwrap();
    assert!(!store.state.instances["order-1"].pending.is_empty());

    // Further lines: each one drives another tick. Nothing happens between
    // them, which is exactly the limit embedded mode has and the reason the
    // unattended claim belongs to a separate process.
    for round in 0..40 {
        if store
            .records
            .iter()
            .any(|record| record.kind == RecordKind::EffectAcked)
        {
            break;
        }
        let mut output = Vec::new();
        serve_session_with(
            Some(&mut store),
            &mut clock,
            Some(&mut executor),
            None,
            Cursor::new(
                format!(
                    "{{\"id\":{},\"jsonrpc\":\"2.0\",\"method\":\"ping\"}}\n",
                    900 + round
                )
                .as_bytes(),
            ),
            &mut output,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::EffectAcked)
            .count(),
        1,
        "the serve process itself journaled the ack — no external executor"
    );
    assert!(
        store
            .records
            .iter()
            .any(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed")),
        "and the advance the table declares"
    );
}

#[test]
fn the_instructions_adjunct_names_a_non_default_mode() {
    let directory = TestDirectory::create("instructions-mode");
    seeded(&directory);

    let mut store = Store::open_read_only(directory.path()).unwrap();
    let mut clock = SystemClock;
    let mut output = Vec::new();
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(initialize_lines().as_bytes()),
        &mut output,
    )
    .unwrap();
    let instructions = responses(&String::from_utf8(output).unwrap())[0]
        .get("result")
        .and_then(|result| result.get("instructions"))
        .and_then(Value::as_str)
        .expect("initialize carries instructions")
        .to_string();
    assert!(instructions.contains("mode=read-only"), "{instructions}");
    assert!(instructions.contains("effect_ack"), "{instructions}");

    // The default mode adds nothing: those instructions are byte-compared by
    // the MCP transcripts.
    let mut writer = open_writer(directory.path());
    let mut output = Vec::new();
    serve_session(
        Some(&mut writer),
        &mut clock,
        Cursor::new(initialize_lines().as_bytes()),
        &mut output,
    )
    .unwrap();
    let default_instructions = responses(&String::from_utf8(output).unwrap())[0]
        .get("result")
        .and_then(|result| result.get("instructions"))
        .and_then(Value::as_str)
        .expect("initialize carries instructions")
        .to_string();
    assert!(
        !default_instructions.contains("mode="),
        "the writer mode is silent"
    );
}

#[test]
fn exclusive_refuses_to_start_beside_another_writer() {
    let directory = TestDirectory::create("exclusive");
    seeded(&directory);
    let handlers = directory.path().join("handlers.json");
    fs::write(&handlers, stub_table_json()).unwrap();
    let held = open_writer(directory.path());

    let output = Command::new(binary())
        .args([
            "--data-dir",
            &directory.path().to_string_lossy(),
            "execute",
            "--exclusive",
            "--handlers",
            &handlers.to_string_lossy(),
        ])
        .output()
        .expect("run fsm execute --exclusive");
    drop(held);
    assert!(!output.status.success());
    let rendered = String::from_utf8_lossy(&output.stderr);
    assert!(rendered.contains("exec/mode"), "{rendered}");
}

#[test]
fn an_exclusive_loop_stops_the_moment_it_is_actually_blocked() {
    // The startup check is a fast failure and inherently raceable — the lock
    // it takes is released again before the loop starts. This is the check
    // that cannot be raced, because it *is* the write: a tick that has
    // something to journal and cannot take the writer ends an exclusive run.
    let directory = TestDirectory::create("exclusive-loop");
    seeded(&directory);
    let mut writer = open_writer(directory.path());
    let mut writer_clock = FixedClock::new(9_000, 1);
    writer
        .send_event_stamp_on(
            &mut writer_clock,
            "order-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();

    // The writer stays open, so every settling tick finds the lock taken.
    let mut clock = FixedClock::new(50_000, 1);
    let mut lines = Vec::new();
    let error = fsm_execute::service::run(
        fsm_execute::service::RunConfig {
            data_dir: directory.path(),
            table: HandlerTable::parse(&stub_table_json()).unwrap(),
            poll_interval_ms: 20,
            contention: fsm_execute::service::Contention::Fail,
        },
        &mut clock,
        &mut |line: &str| lines.push(line.to_string()),
    )
    .expect_err("an exclusive run does not continue past a held writer");
    assert_eq!(error.code, "exec/mode");
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("error exec/store")),
        "the blocked tick says why before the run ends: {lines:?}"
    );
}

#[test]
fn paired_keeps_retrying_instead_of_exiting_when_the_writer_is_held() {
    let directory = TestDirectory::create("paired-contention");
    seeded(&directory);
    let handlers = directory.path().join("handlers.json");
    fs::write(&handlers, stub_table_json()).unwrap();
    let held = open_writer(directory.path());

    let mut child = Command::new(binary())
        .args([
            "--data-dir",
            &directory.path().to_string_lossy(),
            "execute",
            "--handlers",
            &handlers.to_string_lossy(),
            "--poll-interval-ms",
            "50",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run fsm execute");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let alive = child.try_wait().expect("poll the executor").is_none();
    let _ = child.kill();
    let _ = child.wait();
    drop(held);
    assert!(
        alive,
        "contention is ordinary in paired mode: back off, retry, do not exit"
    );
}

#[test]
fn every_mode_announces_itself_on_stderr() {
    let directory = TestDirectory::create("mode-line");
    seeded(&directory);
    let handlers = directory.path().join("handlers.json");
    fs::write(&handlers, stub_table_json()).unwrap();

    for (arguments, expected) in [
        (vec!["serve"], "mode=writer"),
        (vec!["serve", "--read-only"], "mode=read-only"),
    ] {
        let mut argv = vec!["--data-dir", &directory.path().to_str().unwrap()];
        argv.extend(arguments);
        let output = Command::new(binary())
            .args(&argv)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run fsm serve");
        let rendered = String::from_utf8_lossy(&output.stderr);
        assert!(
            rendered.contains(expected),
            "{expected} missing: {rendered}"
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("mode="),
            "the mode line never enters the protocol stream"
        );
    }
}
