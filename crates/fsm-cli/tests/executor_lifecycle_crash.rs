//! Independent pipe/tree observer for the portable lifecycle matrix fixture.
//! Native production cutpoints remain a separate acceptance obligation.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use fsm_core::json::{JsonLimits, Value, parse};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "linux")]
#[path = "executor_lifecycle_crash/native.rs"]
mod native;

struct Fixture {
    directory: PathBuf,
    root: Child,
}

impl Fixture {
    fn start(mode: &str, behavior: &str) -> Self {
        let cache = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache"));
        assert!(!cache.starts_with("/tmp"));
        let directory = cache.join(format!(
            "lifecycle-matrix-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        let root = Command::new(env!("CARGO_BIN_EXE_fsm-lifecycle-fixture"))
            .arg(mode)
            .arg(&directory)
            .arg(behavior)
            .stdin(if mode == "mcp" {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self { directory, root }
    }

    fn release_tree(&self) {
        for role in ["grandchild", "child", "root"] {
            fs::write(self.directory.join(format!("{role}-release")), b"release").unwrap();
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.release_tree();
        if self.root.try_wait().ok().flatten().is_none() {
            let _ = self.root.kill();
            let _ = self.root.wait();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while !self.directory.join("child-retired").is_file() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if self.directory.join("child-retired").is_file() {
            fs::remove_dir_all(&self.directory).unwrap();
        }
    }
}

fn root_exit_retains_descendant_pipes(mode: &str) {
    let mut fixture = Fixture::start(mode, "exit-root");
    let stdout = fixture.root.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            sender
                .send(Some(parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap()))
                .unwrap();
        }
        sender.send(None).unwrap();
    });
    if mode == "mcp" {
        let input = fixture.root.stdin.as_mut().unwrap();
        input
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n")
            .unwrap();
        input.flush().unwrap();
        let response = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(response.get("id"), Some(&Value::Num("1".into())));
        assert_eq!(
            response.get("result").unwrap().get("protocolVersion"),
            Some(&Value::Str("2025-06-18".into()))
        );
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"run\",\"arguments\":{}}}\n").unwrap();
        input.flush().unwrap();
    }
    let response = receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let result = if mode == "mcp" {
        assert_eq!(response.get("id"), Some(&Value::Num("2".into())));
        response
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
    } else {
        &response
    };
    assert_eq!(result.get("ok"), Some(&Value::Bool(true)));
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = fixture.root.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "root failed to exit independently"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success());
    for role in ["child", "grandchild"] {
        assert!(fixture.directory.join(format!("{role}-entered")).is_file());
        assert!(!fixture.directory.join(format!("{role}-retired")).exists());
    }
    assert!(
        matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "root exit must not close descendant-owned pipes"
    );
    fixture.release_tree();
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), None);
    reader.join().unwrap();
    assert!(fixture.directory.join("child-retired").is_file());
    assert!(fixture.directory.join("grandchild-retired").is_file());
}

#[test]
fn process_fixture_root_exit_keeps_descendant_pipes_open_until_release() {
    root_exit_retains_descendant_pipes("process");
}

#[test]
fn mcp_fixture_root_exit_keeps_descendant_pipes_open_until_release() {
    root_exit_retains_descendant_pipes("mcp");
}

fn candidate_result_waits_for_explicit_release(mode: &str) {
    let mut fixture = Fixture::start(mode, "hold-result");
    let stdout = fixture.root.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            sender.send(line.unwrap()).unwrap();
        }
    });
    if mode == "mcp" {
        let input = fixture.root.stdin.as_mut().unwrap();
        input
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n")
            .unwrap();
        input.flush().unwrap();
        let response = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        let response = parse(response.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        assert_eq!(response.get("id"), Some(&Value::Num("1".into())));
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"run\",\"arguments\":{}}}\n").unwrap();
        input.flush().unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.directory.join("root-candidate").is_file() {
        assert!(fixture.root.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "candidate barrier was never reached"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!fixture.directory.join("root-published").exists());
    assert!(!fixture.directory.join("root-retired").exists());
    assert!(
        matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "candidate result escaped its explicit barrier"
    );
    // Release only the root first: descendants remain observable and retain pipes.
    fs::write(fixture.directory.join("root-release"), b"release").unwrap();
    let response = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let response = parse(response.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let result = if mode == "mcp" {
        assert_eq!(response.get("id"), Some(&Value::Num("2".into())));
        response
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
    } else {
        &response
    };
    assert_eq!(result.get("ok"), Some(&Value::Bool(true)));
    assert!(
        fixture.root.try_wait().unwrap().is_none(),
        "root must wait for its original child retirement"
    );
    assert!(!fixture.directory.join("child-retired").exists());
    fixture.release_tree();
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    reader.join().unwrap();
    assert!(fixture.root.wait().unwrap().success());
    for role in ["root", "child", "grandchild"] {
        assert!(fixture.directory.join(format!("{role}-retired")).is_file());
    }
}

#[test]
fn process_candidate_result_is_held_until_explicit_release() {
    candidate_result_waits_for_explicit_release("process");
}

#[test]
fn mcp_candidate_result_is_held_until_explicit_release() {
    candidate_result_waits_for_explicit_release("mcp");
}
