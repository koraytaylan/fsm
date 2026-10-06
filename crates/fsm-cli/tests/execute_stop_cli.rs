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
