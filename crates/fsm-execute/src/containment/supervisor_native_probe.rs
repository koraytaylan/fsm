//! Test-only subprocess entry exercising the public production host adapter.

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::run::native_client::{NativeRequest, NativeRun, NativeRunPhase};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[test]
#[ignore = "invoked only as an unprivileged subprocess of native broker tests"]
fn owned_request() {
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let namespace = std::env::var("FSM_NATIVE_TEST_NAMESPACE").unwrap();
    let binding = std::env::var("FSM_NATIVE_TEST_BINDING")
        .ok()
        .map(|encoded| {
            assert!(encoded.len() <= 8192);
            parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap()
        });
    if let Some(binding) = binding {
        complete(binding);
        return;
    }
    let request = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.native-request/1".into())),
        ("action".into(), Value::Str("execute".into())),
        ("payload".into(), Value::Num("1".into())),
    ]));
    let mut owned = NativeRequest::start(&namespace, 1, &request, Duration::from_secs(30)).unwrap();
    loop {
        assert!(
            owned.poll().unwrap().is_none(),
            "supervisor received result before independent death"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn complete(binding: Value) {
    let timeout = matches!(std::env::var("FSM_NATIVE_TEST_TIMEOUT").as_deref(), Ok("1"));
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let hash = binding
        .get("journal_claim")
        .and_then(Value::as_str)
        .unwrap();
    let negative = matches!(std::env::var("FSM_NATIVE_TEST_CANCEL").as_deref(), Ok("1"))
        || matches!(std::env::var("FSM_NATIVE_TEST_REFUSE").as_deref(), Ok("1"));
    let mut contention = None;
    let mut owned = if negative {
        NativeRun::start(&claim, hash, Duration::from_secs(30)).unwrap()
    } else {
        let path = std::env::var("FSM_NATIVE_TEST_STORE")
            .expect("positive host requires operator-owned store");
        let mut store = fsm_store::store::Store::open(std::path::Path::new(&path)).unwrap();
        let mut pipeline = fsm_execute::run::Pipeline;
        let records = store.records.len();
        let state = store.state.clone();
        assert_eq!(store.current_execution_claim_hash(&claim).unwrap(), hash);
        let mut holder = WriterHolder::start(&path);
        let mut owned = pipeline
            .start_native(&mut store, &claim, Duration::from_secs(30))
            .unwrap();
        assert_eq!(store.records.len(), records);
        assert_eq!(store.state, state);
        // Send binding before the lease barrier, but never dispatch execute
        // on this first poll even if the binding response is already available.
        assert!(owned.poll().unwrap().is_none());
        assert!(matches!(
            owned.progress().phase,
            NativeRunPhase::Binding | NativeRunPhase::Bound
        ));
        drop(store);
        holder.acquire();
        let domain = claim.domain().to_value();
        let authority = std::path::Path::new(&path).parent().unwrap().join(format!(
            "authority-{}",
            domain.get("generation").unwrap().as_num().unwrap()
        ));
        let allocation = domain.get("allocation").unwrap().as_num().unwrap();
        for name in ["launch", "entry", "handoff"] {
            assert_eq!(
                std::fs::symlink_metadata(authority.join(format!("{name}-{allocation}.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
        contention = Some(holder);
        owned
    };
    if negative {
        assert_eq!(owned.progress().phase, NativeRunPhase::Binding);
        assert!(!owned.progress().helper.reaped);
    }
    if matches!(std::env::var("FSM_NATIVE_TEST_CANCEL").as_deref(), Ok("1")) {
        owned.cancel().unwrap();
        match owned.poll() {
            Err(error) => assert!(error.contains("cancelled")),
            Ok(_) => panic!("cancelled run continued polling"),
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owned.reap().unwrap() {
            assert!(
                Instant::now() < deadline,
                "cancelled helper did not reap with EOF"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_retired_uncertain(&owned);
        println!("\nFSM_NATIVE_TEST_CANCELLED");
        return;
    }
    if matches!(std::env::var("FSM_NATIVE_TEST_REFUSE").as_deref(), Ok("1")) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match owned.poll() {
                Err(error) => {
                    assert!(
                        error.contains("binding refused"),
                        "unexpected refusal: {error}"
                    );
                    assert!(owned.poll().is_err());
                    break;
                }
                Ok(None) => {}
                Ok(Some(_)) => panic!("mismatched original hash produced completion"),
            }
            assert!(Instant::now() < deadline, "binding refusal timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owned.reap().unwrap() {
            assert!(
                Instant::now() < deadline,
                "refused helper did not reap with EOF"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_retired_uncertain(&owned);
        println!("\nFSM_NATIVE_TEST_REFUSED");
        return;
    }
    loop {
        if let Some(holder) = &mut contention {
            assert!(
                holder.child.try_wait().unwrap().is_none(),
                "independent writer exited early"
            );
        }
        if let Some(completion) = owned.poll().unwrap() {
            let mut holder = contention
                .take()
                .expect("positive native run requires independent writer contention");
            assert!(holder.child.try_wait().unwrap().is_none());
            let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
            let path = std::path::Path::new(&path);
            match fsm_store::store::Store::open(path) {
                Err(error) => assert_eq!(error.code, "store/lock"),
                Ok(_) => panic!("independent writer did not retain its lease through completion"),
            }
            let snapshot = fsm_store::store::Store::open_read_only(path).unwrap();
            let (instance, effect) = claim.effect();
            assert_eq!(
                snapshot.state.execution.claim_for(instance, effect),
                Some(&claim)
            );
            assert!(
                snapshot
                    .state
                    .execution
                    .stopped_for(instance, effect)
                    .is_none()
            );
            assert!(
                snapshot.state.instances[instance]
                    .pending
                    .iter()
                    .any(|pending| pending == effect)
            );
            assert_eq!(snapshot.current_execution_claim_hash(&claim).unwrap(), hash);
            assert_eq!(
                snapshot.journal.last_hash,
                hash.strip_prefix("sha256:").unwrap()
            );
            let retained = snapshot.state.execution.clone();
            drop(snapshot);
            holder.release();
            let writer = fsm_store::store::Store::open(path).unwrap();
            assert_eq!(writer.state.execution, retained);
            assert_eq!(writer.current_execution_claim_hash(&claim).unwrap(), hash);
            drop(writer);
            assert_eq!(owned.progress().phase, NativeRunPhase::Closed);
            let helper = owned.progress().helper;
            assert!(helper.reaped && helper.stdout_eof && helper.stderr_eof);
            assert_eq!(
                completion.candidate().get("status"),
                Some(&Value::Num(if timeout { "-1" } else { "0" }.into()))
            );
            assert_eq!(
                completion.failure_class(),
                timeout.then_some(fsm_core::record::execution::FailureClass::Timeout)
            );
            assert_eq!(
                completion.stopped_outcome().status(),
                if timeout { "timeout" } else { "ok" }
            );
            if timeout {
                assert_eq!(
                    completion.candidate().get("error").and_then(Value::as_str),
                    Some("exec/timeout")
                );
            }
            assert_eq!(
                completion.stopped_outcome().result(),
                Some(completion.candidate())
            );
            assert!(completion.proof().matches_claim(&claim, hash));
            // Reconstruct a test-only envelope from the checked completion;
            // the independent observer re-authenticates its protected receipt.
            let domain = claim.domain().to_value();
            let receipt = format!(
                "/var/lib/fsm-containment/{}/authority-{}/closure-{}-{}.json",
                domain.get("namespace").and_then(Value::as_str).unwrap(),
                domain.get("generation").and_then(Value::as_num).unwrap(),
                domain.get("allocation").and_then(Value::as_num).unwrap(),
                claim.run_id()
            );
            let result = Value::Obj(BTreeMap::from([
                (
                    "format".into(),
                    Value::Str("fsm.native-run-result/2".into()),
                ),
                ("handler_kind".into(), Value::Str("process".into())),
                ("claim".into(), claim.to_value()),
                ("journal_claim".into(), Value::Str(hash.into())),
                ("receipt".into(), Value::Str(receipt)),
                ("candidate".into(), completion.candidate().clone()),
                (
                    "failure_class".into(),
                    if timeout {
                        Value::Str("timeout".into())
                    } else {
                        Value::Null
                    },
                ),
            ]));
            let response = Value::Obj(BTreeMap::from([
                ("format".into(), Value::Str("fsm.native-response/1".into())),
                ("ok".into(), Value::Bool(true)),
                ("result".into(), result),
            ]));
            println!(
                "\nFSM_NATIVE_TEST_RESPONSE={}",
                std::str::from_utf8(&fsm_core::canon::canon_bytes(&response)).unwrap()
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn assert_retired_uncertain(owned: &NativeRun) {
    let progress = owned.progress();
    assert_eq!(progress.phase, NativeRunPhase::Uncertain);
    assert!(progress.helper.reaped && progress.helper.stdout_eof && progress.helper.stderr_eof);
    assert_eq!(owned.progress(), progress);
}

#[test]
#[ignore = "invoked only by the unprivileged native supervisor"]
fn writer_holder() {
    use std::io::Read;
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    use std::io::Write;
    println!("\nFSM_NATIVE_WRITER_WAITING");
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    std::io::stdin().read_exact(&mut byte).unwrap();
    assert_eq!(byte, [1]);
    let store = fsm_store::store::Store::open(std::path::Path::new(&path)).unwrap();
    println!("\nFSM_NATIVE_WRITER_READY");
    std::io::stdout().flush().unwrap();
    assert_eq!(
        std::io::stdin().read(&mut byte).unwrap(),
        0,
        "independent writer release requires EOF"
    );
    drop(store);
}

struct WriterHolder {
    child: std::process::Child,
    input: Option<std::os::unix::net::UnixStream>,
    output: std::os::unix::net::UnixStream,
}

impl WriterHolder {
    fn start(path: &str) -> Self {
        use std::os::fd::OwnedFd;
        use std::os::unix::net::UnixStream;
        use std::process::{Command, Stdio};
        let (input, child_input) = UnixStream::pair().unwrap();
        let (output, child_output) = UnixStream::pair().unwrap();
        output.set_nonblocking(true).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "authority::allocator::native_tests::supervisor_probe::writer_holder",
                "--ignored",
                "--nocapture",
                "--color",
                "never",
            ])
            .env("FSM_NATIVE_TEST_STORE", path)
            .stdin(Stdio::from(OwnedFd::from(child_input)))
            .stdout(Stdio::from(OwnedFd::from(child_output)))
            .stderr(Stdio::null());
        let child = command.spawn().unwrap();
        drop(command);
        let mut owned = Self {
            child,
            input: Some(input),
            output,
        };
        owned.wait_marker(b"FSM_NATIVE_WRITER_WAITING");
        owned
    }

    fn acquire(&mut self) {
        use std::io::Write;
        self.input.as_mut().unwrap().write_all(&[1]).unwrap();
        self.wait_marker(b"FSM_NATIVE_WRITER_READY");
    }

    fn wait_marker(&mut self, marker: &[u8]) {
        use std::io::Read;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut bytes = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            match self.output.read(&mut buffer) {
                Ok(0) => panic!("independent writer output closed before readiness"),
                Ok(read) => {
                    bytes.extend_from_slice(&buffer[..read]);
                    assert!(bytes.len() <= 8192);
                    if bytes
                        .split(|byte| *byte == b'\n')
                        .any(|line| line == marker)
                    {
                        break;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("independent writer readiness read failed: {error}"),
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "independent writer did not reach barrier"
            );
            assert!(
                Instant::now() < deadline,
                "independent writer readiness deadline"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn release(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "independent writer release failed");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "independent writer release deadline"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for WriterHolder {
    fn drop(&mut self) {
        self.input.take();
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.child.kill();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}
