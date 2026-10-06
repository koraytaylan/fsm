//! Real stop binary and actual writer ownership; no fabricated native proof.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use fsm_cli::local_control::LocalControlEndpoint;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::{
    config::HandlerTable,
    service::{OwnedNativeExecutor, ShutdownMode},
};
use fsm_store::{clock::FixedClock, store::Store};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    data: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let cache = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache"));
        assert!(!cache.starts_with("/tmp"));
        let root = cache.join(format!(
            "sc{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let data = root.join("data");
        Self { root, data }
    }
    fn driver(&self) -> OwnedNativeExecutor {
        OwnedNativeExecutor::new(Store::open(&self.data).unwrap(), HandlerTable::default()).unwrap()
    }
    fn command(&self, mode: &str, timeout: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
        command
            .args(["--json", "--data-dir"])
            .arg(&self.data)
            .args(["execute", "stop", "--control-dir"])
            .arg(&self.root)
            .args(["--mode", mode, "--timeout-ms", timeout])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn json(bytes: &[u8]) -> Value {
    parse(bytes, &JsonLimits::DEFAULT).unwrap()
}
fn bounded(command: &mut Command, mut observe: impl FnMut()) -> Output {
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        observe();
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("stop binary exceeded observation deadline");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn stop_binary_returns_actual_stopped_after_original_owner_releases_writer() {
    let maximum = fsm_execute::config::MAX_TIMEOUT_MS.to_string();
    for (mode, timeout) in [("abort", "1000"), ("drain", maximum.as_str())] {
        let fixture = Fixture::new();
        let mut driver = fixture.driver();
        let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
        let output = bounded(&mut fixture.command(mode, timeout), || {
            if driver.control().report().admission_closed {
                driver.poll(&mut FixedClock::new(0, 1), 0);
            }
        });
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let report = json(&output.stdout);
        assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
        assert_eq!(report.get("writer_released"), Some(&Value::Bool(true)));
        drop(Store::open(&fixture.data).unwrap());
        assert!(endpoint.close(1000).unwrap());
    }
}

#[test]
fn stop_binary_reports_original_deadline_uncertainty_without_acquiring_writer() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    driver.control().stop(ShutdownMode::Drain, 50).unwrap();
    let output = bounded(&mut fixture.command("abort", "1000"), || {});
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = json(&output.stderr);
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("exec/inflight_deferred")
    );
    let report = error.get("details").unwrap();
    assert_eq!(
        report.get("phase").and_then(Value::as_str),
        Some("uncertain")
    );
    assert_eq!(report.get("admission_closed"), Some(&Value::Bool(true)));
    assert_eq!(report.get("writer_released"), Some(&Value::Bool(false)));
    assert!(Store::open(&fixture.data).is_err());
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn missing_endpoint_does_not_create_data_or_confirm_admission() {
    let fixture = Fixture::new();
    let output = bounded(&mut fixture.command("abort", "1000"), || {});
    assert_eq!(output.status.code(), Some(1));
    let error = json(&output.stderr);
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("exec/inflight_deferred")
    );
    let facts = error.get("details").unwrap();
    assert_eq!(facts.get("admission_closed"), Some(&Value::Null));
    assert_eq!(facts.get("writer_released"), Some(&Value::Null));
    assert!(!fixture.data.exists());
}

#[test]
fn paired_stop_binary_does_not_acquire_or_release_another_actors_writer() {
    use fsm_execute::service::PairedNativeExecutor;
    let fixture = Fixture::new();
    let writer = Store::open(&fixture.data).unwrap();
    let mut driver = PairedNativeExecutor::new(&fixture.data, HandlerTable::default()).unwrap();
    let endpoint = LocalControlEndpoint::publish_paired(&fixture.root, &driver).unwrap();
    let output = bounded(&mut fixture.command("abort", "1000"), || {
        if driver.control().report().admission_closed {
            driver.poll(&mut FixedClock::new(0, 1), 0);
        }
    });
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output.stdout);
    assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
    assert_eq!(report.get("writer_released"), Some(&Value::Bool(true)));
    assert!(Store::open(&fixture.data).is_err());
    assert!(endpoint.close(1000).unwrap());
    drop(writer);
    drop(Store::open(&fixture.data).unwrap());
}

#[test]
fn invalid_stop_arguments_refuse_before_closing_admission() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let above_maximum = (fsm_execute::config::MAX_TIMEOUT_MS + 1).to_string();
    for (mode, timeout) in [
        ("invalid", "1000"),
        ("abort", "0"),
        ("abort", "-1"),
        ("abort", "9223372036854775807"),
        ("abort", above_maximum.as_str()),
    ] {
        let output = bounded(&mut fixture.command(mode, timeout), || {});
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(
            json(&output.stderr).get("code").and_then(Value::as_str),
            Some("args")
        );
        assert!(!driver.control().report().admission_closed);
    }
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn production_binary_stops_and_removes_endpoint_while_another_writer_is_held() {
    struct Owner(std::process::Child);
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for mode in ["drain", "abort"] {
        let fixture = Fixture::new();
        let writer = Store::open(&fixture.data).unwrap();
        let handlers = fixture.root.join("handlers.json");
        fs::write(
            &handlers,
            br#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["operator"]}"#,
        )
        .unwrap();
        let diagnostic = fixture.root.join("owner-stderr");
        let mut owner = Owner(
            Command::new(env!("CARGO_BIN_EXE_fsm"))
                .arg("--data-dir")
                .arg(&fixture.data)
                .args(["execute", "--handlers"])
                .arg(&handlers)
                .arg("--control-dir")
                .arg(&fixture.root)
                .args(["--poll-interval-ms", "60000"])
                .stdout(Stdio::null())
                .stderr(Stdio::from(fs::File::create(&diagnostic).unwrap()))
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        let directory = loop {
            if let Some(path) = fs::read_dir(&fixture.root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("c-")
                        && path.join("identity").exists()
                })
            {
                break path;
            }
            assert!(
                owner.0.try_wait().unwrap().is_none(),
                "production owner exited before publication: {}",
                fs::read_to_string(&diagnostic).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "production owner did not publish"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        let output = bounded(&mut fixture.command(mode, "1000"), || {});
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = json(&output.stdout);
        assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
        assert_eq!(report.get("writer_released"), Some(&Value::Bool(true)));
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = owner.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "production owner did not exit after stop"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(status.success());
        assert!(!directory.exists());
        assert!(Store::open(&fixture.data).is_err());
        drop(writer);
    }
}

#[test]
fn production_broken_stdout_preserves_failure_and_separate_cleanup_facts() {
    let fixture = Fixture::new();
    let mut writer = Store::open(&fixture.data).unwrap();
    let definition = fsm_core::json::parse(br#"{"format":"fsm.machine/1","name":"quiet","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"done"}]}"#, &fsm_core::json::JsonLimits::DEFAULT).unwrap();
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    writer
        .define_machine_on(&mut clock, definition, false, false)
        .unwrap();
    writer
        .create_instance_ctx_on(
            &mut clock,
            "quiet",
            "instance",
            "create",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    let handlers = fixture.root.join("handlers.json");
    fs::write(
        &handlers,
        br#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["operator"]}"#,
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&fixture.data)
        .args(["execute", "--handlers"])
        .arg(&handlers)
        .arg("--control-dir")
        .arg(&fixture.root)
        .args(["--poll-interval-ms", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close the actual pipe reader; writer-contention diagnostics hit BrokenPipe.
    drop(child.stdout.take());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("broken stdout suspended the production owner");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let error = json(
        stderr
            .lines()
            .find(|line| line.starts_with('{'))
            .unwrap()
            .as_bytes(),
    );
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("exec/inflight_deferred")
    );
    let details = error.get("details").unwrap();
    assert_eq!(
        details.get("phase").and_then(Value::as_str),
        Some("stopped")
    );
    for field in [
        "admission_closed",
        "inventory_complete",
        "helpers_retired",
        "writer_released",
        "endpoint_removed",
    ] {
        assert_eq!(details.get(field), Some(&Value::Bool(true)), "{field}");
    }
    assert_eq!(details.get("unresolved_run_ids"), Some(&Value::Arr(vec![])));
    assert_eq!(
        details
            .get("unclaimed_reservations")
            .and_then(Value::as_str),
        Some("0")
    );
    assert_eq!(details.get("initiating_details"), Some(&Value::Null));
    assert_eq!(details.get("endpoint_cleanup_error"), Some(&Value::Null));
    assert!(details.get("output_drained").is_some());
    assert!(
        details
            .get("dropped_lines")
            .and_then(Value::as_str)
            .is_some()
    );
    assert!(!fs::read_dir(&fixture.root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("c-")
    }));
    assert!(Store::open(&fixture.data).is_err());
    drop(writer);
}

#[test]
fn production_embedded_stdio_stops_with_input_open_and_quiet() {
    struct Owner(std::process::Child);
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for mode in ["drain", "abort"] {
        let fixture = Fixture::new();
        drop(Store::open(&fixture.data).unwrap());
        let handlers = fixture.root.join("handlers.json");
        fs::write(
            &handlers,
            br#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["operator"]}"#,
        )
        .unwrap();
        let mut owner = Owner(
            Command::new(env!("CARGO_BIN_EXE_fsm"))
                .env("HOME", &fixture.root)
                .arg("--data-dir")
                .arg(&fixture.data)
                .args(["serve", "--execute", "--handlers"])
                .arg(&handlers)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let control_root = fixture.root.join(".cache/fsm/control");
        let deadline = Instant::now() + Duration::from_secs(3);
        let endpoint = loop {
            if let Ok(entries) = fs::read_dir(&control_root)
                && let Some(path) = entries
                    .map(|entry| entry.unwrap().path())
                    .find(|path| path.join("identity").exists())
            {
                break path;
            }
            assert!(
                owner.0.try_wait().unwrap().is_none(),
                "native stdio owner exited before publication"
            );
            assert!(
                Instant::now() < deadline,
                "native stdio owner did not publish"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(Store::open(&fixture.data).is_err());
        let mut stop = Command::new(env!("CARGO_BIN_EXE_fsm"));
        stop.args(["--json", "--data-dir"])
            .arg(&fixture.data)
            .args(["execute", "stop", "--control-dir"])
            .arg(&control_root)
            .args(["--mode", mode, "--timeout-ms", "1000"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = bounded(&mut stop, || {});
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = json(&output.stdout);
        assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
        assert_eq!(report.get("writer_released"), Some(&Value::Bool(true)));
        // Never send or close stdin: explicit control must wake the owner itself.
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = owner.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "quiet stdin suspended native shutdown"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(status.success());
        assert!(owner.0.stdin.is_some());
        assert!(!endpoint.exists());
        drop(Store::open(&fixture.data).unwrap());
    }
}

#[test]
fn production_embedded_contention_preserves_diagnosis_without_publication() {
    let fixture = Fixture::new();
    let writer = Store::open(&fixture.data).unwrap();
    let records = writer.records.clone();
    let handlers = fixture.root.join("handlers.json");
    fs::write(
        &handlers,
        br#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["operator"]}"#,
    )
    .unwrap();
    let input = fixture.root.join("input.jsonl");
    fs::write(&input, b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
    command
        .env("HOME", &fixture.root)
        .arg("--data-dir")
        .arg(&fixture.data)
        .args(["serve", "--execute", "--handlers"])
        .arg(&handlers)
        .stdin(Stdio::from(fs::File::open(&input).unwrap()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = bounded(&mut command, || {});
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reply = json(
        output
            .stdout
            .split(|byte| *byte == b'\n')
            .find(|line| !line.is_empty())
            .unwrap(),
    );
    let instructions = reply
        .get("result")
        .unwrap()
        .get("instructions")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(instructions.contains("contended"));
    assert!(instructions.contains("healthy and busy"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("read-only (writer held elsewhere)"));
    assert!(!fixture.root.join(".cache/fsm/control").exists());
    assert!(Store::open(&fixture.data).is_err());
    assert_eq!(
        Store::open_read_only(&fixture.data).unwrap().records,
        records
    );
    drop(writer);
}
