//! Actual Unix transport and writer locks, without fabricated native proofs.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use fsm_cli::local_control::{LocalControlEndpoint, stop};
use fsm_core::json::{JsonLimits, Value, parse, write_canonical};
use fsm_execute::{
    config::HandlerTable,
    service::{ExecutorPhase, OwnedNativeExecutor, PairedNativeExecutor, ShutdownMode},
};
use fsm_store::{clock::FixedClock, store::Store};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::DirBuilderExt, net::UnixStream},
    path::PathBuf,
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
            "lc{}-{}",
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
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn request(endpoint: &LocalControlEndpoint, mode: &str, timeout_ms: i64) -> Value {
    let mut value = parse(
        &fs::read(endpoint.directory().join("identity")).unwrap(),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let Value::Obj(fields) = &mut value else {
        panic!("object identity")
    };
    fields.insert("format".into(), Value::Str("fsm.executor-control/1".into()));
    fields.insert("mode".into(), Value::Str(mode.into()));
    fields.insert("timeout_ms".into(), Value::Num(timeout_ms.to_string()));
    value
}
fn send(endpoint: &LocalControlEndpoint, value: &Value, padding: usize) -> UnixStream {
    let mut stream = UnixStream::connect(endpoint.directory().join("s")).unwrap();
    let mut bytes = Vec::new();
    write_canonical(value, &mut bytes);
    bytes.resize(bytes.len() + padding, b' ');
    bytes.push(b'\n');
    stream.write_all(&bytes).unwrap();
    stream
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !predicate() {
        assert!(Instant::now() < deadline, "observation deadline");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn control_responds_while_original_journal_writer_is_held() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let control = driver.control();
    // No driver polls: writer remains held throughout the independent response.
    let mut stream = send(&endpoint, &request(&endpoint, "abort", 50), 0);
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let value = parse(&response, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        value.get("phase").and_then(Value::as_str),
        Some("uncertain")
    );
    assert_eq!(value.get("admission_closed"), Some(&Value::Bool(true)));
    assert_eq!(value.get("writer_released"), Some(&Value::Bool(false)));
    assert!(Store::open(&fixture.data).is_err());
    assert_eq!(control.report().phase, ExecutorPhase::Uncertain);
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn later_abort_escalates_after_more_than_connection_cap_long_drains() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let control = driver.control();
    let drain = request(&endpoint, "drain", 5000);
    let mut connections = Vec::new();
    for _ in 0..72 {
        connections.push(send(&endpoint, &drain, 0));
    }
    wait_for(|| control.report().phase == ExecutorPhase::Draining);
    let original_deadline = control.stop(ShutdownMode::Drain, 5000).unwrap().deadline();
    let _abort = send(&endpoint, &request(&endpoint, "abort", 1000), 0);
    wait_for(|| control.report().phase == ExecutorPhase::Stopping);
    assert_eq!(
        control.stop(ShutdownMode::Drain, 1000).unwrap().deadline(),
        original_deadline
    );
    assert!(control.report().admission_closed);
    assert!(!control.report().writer_released);
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn exact_request_byte_limit_admits_and_limit_plus_one_does_not_close_admission() {
    for over in [true, false] {
        let fixture = Fixture::new();
        let mut driver = fixture.driver();
        let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
        let value = request(&endpoint, "abort", 50);
        let mut bytes = Vec::new();
        write_canonical(&value, &mut bytes);
        let padding = 1024 + usize::from(over) - bytes.len();
        let mut stream = send(&endpoint, &value, padding);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        assert_eq!(driver.control().report().admission_closed, !over);
        if !over {
            assert_eq!(
                parse(&response, &JsonLimits::DEFAULT)
                    .unwrap()
                    .get("phase")
                    .and_then(Value::as_str),
                Some("uncertain")
            );
        }
        assert!(endpoint.close(1000).unwrap());
    }
}

#[test]
fn stale_incarnation_unknown_fields_and_invalid_bounds_refuse_before_admission() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    for refusal in 0..7 {
        let mut value = request(&endpoint, "abort", 1000);
        let Value::Obj(fields) = &mut value else {
            unreachable!()
        };
        match refusal {
            0 => {
                fields.insert("incarnation".into(), Value::Str("0".repeat(64)));
            }
            1 => {
                fields.insert("extra".into(), Value::Null);
            }
            2 => {
                fields.insert("timeout_ms".into(), Value::Num("0".into()));
            }
            3 => {
                fields.insert("timeout_ms".into(), Value::Num("-1".into()));
            }
            4 => {
                fields.insert("timeout_ms".into(), Value::Num(i64::MAX.to_string()));
            }
            5 => {
                fields.insert("store_device".into(), Value::Num(u64::MAX.to_string()));
            }
            _ => {
                fields.insert("store_inode".into(), Value::Num(u64::MAX.to_string()));
            }
        }
        let mut stream = send(&endpoint, &value, 0);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        assert!(
            parse(&response, &JsonLimits::DEFAULT)
                .unwrap()
                .get("error")
                .is_some()
        );
        assert!(!driver.control().report().admission_closed);
        assert!(driver.store_mut().is_some());
    }
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn discovered_stop_reports_actual_writer_release_after_owner_poll() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let root = fixture.root.clone();
    let data = fixture.data.clone();
    let caller = std::thread::spawn(move || stop(&root, &data, ShutdownMode::Abort, 1000));
    wait_for(|| driver.control().report().admission_closed);
    driver.poll(&mut FixedClock::new(0, 1), 0);
    let response = caller.join().unwrap().unwrap();
    assert_eq!(
        response.get("phase").and_then(Value::as_str),
        Some("stopped")
    );
    assert_eq!(response.get("writer_released"), Some(&Value::Bool(true)));
    drop(Store::open(&fixture.data).unwrap());
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn ambiguous_original_endpoints_refuse_without_stopping_either_control() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let first = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let second = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let error = stop(&fixture.root, &fixture.data, ShutdownMode::Abort, 1000).unwrap_err();
    assert!(error.to_string().contains("ambiguous"));
    assert!(!driver.control().report().admission_closed);
    assert!(first.close(1000).unwrap());
    assert!(second.close(1000).unwrap());
}

#[test]
fn silent_clients_do_not_prevent_later_abort_admission() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let mut silent = Vec::new();
    for _ in 0..72 {
        silent.push(UnixStream::connect(endpoint.directory().join("s")).unwrap());
    }
    let _abort = send(&endpoint, &request(&endpoint, "abort", 1000), 0);
    wait_for(|| driver.control().report().phase == ExecutorPhase::Stopping);
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn cleanup_refuses_replaced_file_and_preserves_replacement() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let identity = endpoint.directory().join("identity");
    // Retain original inode elsewhere to prevent allocator inode reuse.
    fs::rename(&identity, fixture.root.join("retained-identity")).unwrap();
    fs::write(&identity, b"replacement").unwrap();
    assert!(!endpoint.close(50).unwrap());
    assert_eq!(fs::read(identity).unwrap(), b"replacement");
    assert!(!driver.control().report().admission_closed);
}

#[test]
fn nonprivate_or_symlink_root_refuses_before_endpoint_publication() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(LocalControlEndpoint::publish(&fixture.root, &mut driver).is_err());
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
    let alias = fixture.root.join("alias");
    symlink(&fixture.root, &alias).unwrap();
    assert!(LocalControlEndpoint::publish(&alias, &mut driver).is_err());
    assert!(!driver.control().report().admission_closed);
}

#[test]
fn invalid_client_and_close_bounds_leave_endpoint_and_control_live() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    for timeout in [-1, 0, i64::MAX] {
        assert!(stop(&fixture.root, &fixture.data, ShutdownMode::Abort, timeout).is_err());
        assert!(endpoint.close(timeout).is_err());
        assert!(endpoint.directory().join("s").exists());
        assert!(!driver.control().report().admission_closed);
    }
    assert!(endpoint.close(fsm_execute::config::MAX_TIMEOUT_MS).unwrap());
}

#[test]
fn partial_request_expires_without_closing_native_admission() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let mut stream = UnixStream::connect(endpoint.directory().join("s")).unwrap();
    stream.write_all(b"{").unwrap();
    // Real transport budget is intentionally exercised, independently of the
    // injected engine clock; the peer receives EOF without completing a frame.
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut byte = [0u8; 1];
    assert_eq!(stream.read(&mut byte).unwrap(), 0);
    assert!(!driver.control().report().admission_closed);
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn maximum_timeout_is_accepted_without_renewing_it_on_abort() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let control = driver.control();
    let _drain = send(
        &endpoint,
        &request(&endpoint, "drain", fsm_execute::config::MAX_TIMEOUT_MS),
        0,
    );
    wait_for(|| control.report().phase == ExecutorPhase::Draining);
    let deadline = control.stop(ShutdownMode::Drain, 1).unwrap().deadline();
    let _abort = send(&endpoint, &request(&endpoint, "abort", 1), 0);
    wait_for(|| control.report().phase == ExecutorPhase::Stopping);
    assert_eq!(
        control.stop(ShutdownMode::Abort, 1).unwrap().deadline(),
        deadline
    );
    assert!(endpoint.close(1000).unwrap());
}

#[test]
fn paired_publication_refuses_replaced_physical_directory_without_endpoint_files() {
    let fixture = Fixture::new();
    let writer = Store::open(&fixture.data).unwrap();
    let driver = PairedNativeExecutor::new(&fixture.data, HandlerTable::default()).unwrap();
    let original = fixture.root.join("original");
    fs::rename(&fixture.data, &original).unwrap();
    drop(Store::open(&fixture.data).unwrap());
    let published = LocalControlEndpoint::publish_paired(&fixture.root, &driver);
    if let Ok(endpoint) = &published {
        assert!(endpoint.close(1000).unwrap());
    }
    fs::remove_dir_all(&fixture.data).unwrap();
    fs::rename(&original, &fixture.data).unwrap();
    assert!(published.is_err());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    assert!(!driver.control().report().admission_closed);
    assert!(Store::open(&fixture.data).is_err());
    drop(writer);
}

#[test]
fn paired_publication_refuses_stopped_actor_without_new_incarnation() {
    let fixture = Fixture::new();
    let writer = Store::open(&fixture.data).unwrap();
    let mut driver = PairedNativeExecutor::new(&fixture.data, HandlerTable::default()).unwrap();
    let request = driver.control().stop(ShutdownMode::Abort, 1000).unwrap();
    driver.poll(&mut FixedClock::new(0, 1), 0);
    assert_eq!(request.wait().phase, ExecutorPhase::Stopped);
    let published = LocalControlEndpoint::publish_paired(&fixture.root, &driver);
    if let Ok(endpoint) = &published {
        assert!(endpoint.close(1000).unwrap());
    }
    assert!(published.is_err());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    assert!(Store::open(&fixture.data).is_err());
    drop(writer);
}

#[test]
fn expired_cleanup_deadline_stops_transport_without_waiting_for_removal() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let identity = endpoint.directory().join("identity");
    fs::rename(&identity, endpoint.directory().join("original-identity")).unwrap();
    fs::write(&identity, b"replacement").unwrap();
    let begin = Instant::now();
    let removed = endpoint.close_until(begin).unwrap();
    let elapsed = begin.elapsed();
    // Actual replacement-preserving cleanup cannot confirm complete removal.
    assert!(!removed);
    assert!(elapsed < Duration::from_millis(100));
    wait_for(|| !endpoint.directory().join("s").exists());
    assert_eq!(fs::read(&identity).unwrap(), b"replacement");
    assert!(driver.store_mut().is_some());
}

#[test]
fn excessive_cleanup_deadline_refuses_before_closing_transport() {
    let fixture = Fixture::new();
    let mut driver = fixture.driver();
    let endpoint = LocalControlEndpoint::publish(&fixture.root, &mut driver).unwrap();
    let future =
        Instant::now() + Duration::from_millis(fsm_execute::config::MAX_TIMEOUT_MS as u64 + 1000);
    assert!(endpoint.close_until(future).is_err());
    let _request = send(&endpoint, &request(&endpoint, "drain", 1000), 0);
    wait_for(|| driver.control().report().phase == ExecutorPhase::Draining);
    assert!(endpoint.close(1000).unwrap());
    assert!(driver.store_mut().is_some());
}
