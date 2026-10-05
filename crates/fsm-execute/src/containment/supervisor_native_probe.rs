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
        let owned = pipeline
            .start_native(&mut store, &claim, Duration::from_secs(30))
            .unwrap();
        assert_eq!(store.records.len(), records);
        assert_eq!(store.state, state);
        drop(store);
        contention = Some(WriterHolder::start(&path));
        owned
    };
    assert_eq!(owned.progress().phase, NativeRunPhase::Binding);
    assert!(!owned.progress().helper.reaped);
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
            holder.release();
            assert_eq!(owned.progress().phase, NativeRunPhase::Closed);
            let helper = owned.progress().helper;
            assert!(helper.reaped && helper.stdout_eof && helper.stderr_eof);
            assert_eq!(
                completion.candidate().get("status"),
                Some(&Value::Num("0".into()))
            );
            assert_eq!(completion.failure_class(), None);
            assert_eq!(completion.stopped_outcome().status(), "ok");
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
                ("failure_class".into(), Value::Null),
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
    let store = fsm_store::store::Store::open(std::path::Path::new(&path)).unwrap();
    println!("\nFSM_NATIVE_WRITER_READY");
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    let _ = std::io::stdin().read(&mut byte).unwrap();
    drop(store);
}

struct WriterHolder {
    child: std::process::Child,
    input: Option<std::os::unix::net::UnixStream>,
    output: std::os::unix::net::UnixStream,
}

impl WriterHolder {
    fn start(path: &str) -> Self {
        use std::io::Read;
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
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut bytes = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            match owned.output.read(&mut buffer) {
                Ok(0) => panic!("independent writer output closed before readiness"),
                Ok(read) => {
                    bytes.extend_from_slice(&buffer[..read]);
                    assert!(bytes.len() <= 8192);
                    if bytes
                        .split(|byte| *byte == b'\n')
                        .any(|line| line == b"FSM_NATIVE_WRITER_READY")
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
                owned.child.try_wait().unwrap().is_none(),
                "independent writer did not acquire lease"
            );
            assert!(
                Instant::now() < deadline,
                "independent writer readiness deadline"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        owned
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
