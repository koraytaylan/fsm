//! Test-only subprocess entry exercising the public production host adapter.

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::run::native_client::{NativeExecution, NativeRequest, NativeRun, NativeRunPhase};
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
    if negative {
        reject_run(&claim, hash);
        return;
    }
    let (mut owned, mut contention) = {
        let path = std::env::var("FSM_NATIVE_TEST_STORE")
            .expect("positive host requires operator-owned store");
        let mut store = fsm_store::store::Store::open(std::path::Path::new(&path)).unwrap();
        let mut pipeline = fsm_execute::run::Pipeline;
        let records = store.records.len();
        let state = store.state.clone();
        assert_eq!(store.current_execution_claim_hash(&claim).unwrap(), hash);
        // A claimed allocation without a recorded completion must stay
        // uncertain; recovery cannot bind or launch it as a fallback.
        let mut missing = pipeline
            .recover_native(&store, &claim, Duration::from_secs(3))
            .unwrap();
        assert_eq!(missing.progress().phase, NativeRunPhase::Recovering);
        loop {
            match missing.poll() {
                Err(_) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) => panic!("missing completion fabricated a result"),
            }
        }
        assert!(missing.poll().is_err());
        assert!(missing.reap().unwrap());
        assert_retired_uncertain(&missing);
        assert_eq!(store.records.len(), records);
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
        // Refused startup retains the installed owner and permits no implicit
        // second bind, even when a healthy writer becomes available afterwards.
        let mut refused = NativeExecution::retain_uncertain(&claim);
        let mut snapshot =
            fsm_store::store::Store::open_read_only(std::path::Path::new(&path)).unwrap();
        assert_eq!(
            refused
                .start_retained(&mut snapshot, Duration::from_secs(30))
                .unwrap_err()
                .code,
            "exec/mode"
        );
        drop(snapshot);
        assert_eq!(
            refused
                .start_retained(&mut store, Duration::from_secs(30))
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert!(refused.progress().retained);
        assert!(refused.progress().helper.is_none());
        assert_eq!(store.current_execution_claim_hash(&claim).unwrap(), hash);
        let mut holder = WriterHolder::start(&path);
        let mut owned = if timeout {
            NativeExecution::start(&mut store, &claim, Duration::from_secs(30)).unwrap()
        } else {
            let mut installed = NativeExecution::retain_uncertain(&claim);
            installed
                .start_retained(&mut store, Duration::from_secs(30))
                .unwrap();
            installed
        };
        assert_eq!(
            owned
                .start_retained(&mut store, Duration::from_secs(30))
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert_eq!(store.records.len(), records);
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
        // Send binding before the lease barrier, but never dispatch execute
        // on this first poll even if the binding response is already available.
        assert!(!owned.observe().unwrap());
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
        let competitor = std::env::var("FSM_NATIVE_TEST_COMPETING_DOMAIN").unwrap();
        let competitor = parse(competitor.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        let allocation = competitor.get("allocation").unwrap().as_num().unwrap();
        for name in ["binding", "launch", "entry", "handoff"] {
            assert_eq!(
                std::fs::symlink_metadata(authority.join(format!("{name}-{allocation}.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound,
                "competing executor created {name}",
            );
        }
        if !timeout {
            let deadline = Instant::now() + Duration::from_secs(3);
            while owned.progress().phase != NativeRunPhase::Bound {
                assert!(!owned.observe().unwrap());
                assert!(Instant::now() < deadline, "installed binding deadline");
                std::thread::sleep(Duration::from_millis(5));
            }
            for _ in 0..3 {
                assert!(!owned.observe().unwrap());
                assert_eq!(owned.progress().phase, NativeRunPhase::Bound);
            }
            let mut snapshot =
                fsm_store::store::Store::open_read_only(std::path::Path::new(&path)).unwrap();
            assert_eq!(
                owned.launch_bound(&mut snapshot).unwrap_err().code,
                "exec/mode"
            );
            assert_eq!(snapshot.records.len(), records);
            assert!(fsm_store::snapshot::store_states_eq(
                &snapshot.state,
                &state
            ));
            assert_eq!(owned.progress().phase, NativeRunPhase::Bound);
            let original = domain.get("allocation").unwrap().as_num().unwrap();
            for name in ["launch", "entry", "handoff"] {
                assert_eq!(
                    std::fs::symlink_metadata(authority.join(format!("{name}-{original}.json")))
                        .unwrap_err()
                        .kind(),
                    std::io::ErrorKind::NotFound
                );
            }
            drop(snapshot);
            holder.release();
            let mut writer = fsm_store::store::Store::open(std::path::Path::new(&path)).unwrap();
            owned.launch_bound(&mut writer).unwrap();
            assert_eq!(
                owned.launch_bound(&mut writer).unwrap_err().code,
                "exec/inflight_deferred"
            );
            assert_eq!(writer.records.len(), records);
            assert!(fsm_store::snapshot::store_states_eq(&writer.state, &state));
            drop(writer);
            holder = WriterHolder::start(&path);
            holder.acquire();
        }
        (owned, Some(holder))
    };
    loop {
        if let Some(holder) = &mut contention {
            assert!(
                holder.child.try_wait().unwrap().is_none(),
                "independent writer exited early"
            );
        }
        if owned.observe().unwrap() {
            assert!(owned.progress().retained);
            let completion = owned.completion().unwrap();
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
            let mut snapshot = fsm_store::store::Store::open_read_only(path).unwrap();
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
            let mut recovery =
                NativeExecution::recover(&snapshot, &claim, Duration::from_secs(3)).unwrap();
            assert_eq!(recovery.progress().phase, NativeRunPhase::Recovering);
            loop {
                assert!(
                    holder.child.try_wait().unwrap().is_none(),
                    "writer exited during native recovery"
                );
                if recovery.observe().unwrap() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(recovery.progress().phase, NativeRunPhase::Closed);
            let helper = recovery.progress().helper.unwrap();
            assert!(helper.reaped && helper.stdout_eof && helper.stderr_eof);
            assert!(recovery.progress().retained);
            let recovered = recovery.completion().unwrap();
            assert_eq!(recovered.candidate(), completion.candidate());
            assert_eq!(recovered.handler(), completion.handler());
            assert_eq!(recovered.failure_class(), completion.failure_class());
            assert_eq!(recovered.stopped_outcome(), completion.stopped_outcome());
            assert!(recovered.proof().matches_claim(&claim, hash));
            assert_eq!(snapshot.state.execution, retained);
            assert_eq!(snapshot.current_execution_claim_hash(&claim).unwrap(), hash);
            assert!(recovery.observe().unwrap(), "retained completion was lost");
            assert!(recovery.progress().retained);
            assert_eq!(
                recovery
                    .settle(
                        &mut snapshot,
                        &mut fsm_store::clock::FixedClock::new(1000, 1)
                    )
                    .unwrap_err()
                    .code,
                "exec/mode"
            );
            assert!(recovery.progress().retained);
            assert_eq!(snapshot.state.execution, retained);
            drop(snapshot);
            holder.release();
            let writer = fsm_store::store::Store::open(path).unwrap();
            assert_eq!(writer.state.execution, retained);
            assert_eq!(writer.current_execution_claim_hash(&claim).unwrap(), hash);
            drop(writer);
            assert_eq!(owned.progress().phase, NativeRunPhase::Closed);
            let helper = owned.progress().helper.unwrap();
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
                    Value::Str("fsm.native-run-result/3".into()),
                ),
                ("handler_kind".into(), Value::Str("process".into())),
                (
                    "handler_contract".into(),
                    completion.handler().contract_value(),
                ),
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
            emit(format_args!(
                "\nFSM_NATIVE_TEST_RESPONSE={}",
                std::str::from_utf8(&fsm_core::canon::canon_bytes(&response)).unwrap()
            ));
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
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    use std::io::Read;
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    use std::io::Write;
    emit(format_args!("\nFSM_NATIVE_WRITER_WAITING"));
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    std::io::stdin().read_exact(&mut byte).unwrap();
    assert_eq!(byte, [1]);
    let mut store = fsm_store::store::Store::open(std::path::Path::new(&path)).unwrap();
    let binding = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    assert!(binding.len() <= 8192);
    let binding = parse(binding.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let domain = std::env::var("FSM_NATIVE_TEST_COMPETING_DOMAIN").unwrap();
    assert!(domain.len() <= 8192);
    let domain = fsm_core::record::execution::NativeDomain::from_value(
        &parse(domain.as_bytes(), &JsonLimits::DEFAULT).unwrap(),
    )
    .unwrap();
    assert_ne!(domain, *claim.domain());
    let material = claim.to_value();
    let retry =
        fsm_core::record::execution::RetryPolicy::from_value(material.get("retry").unwrap())
            .unwrap();
    let records = store.records.len();
    let state = store.state.clone();
    let head = store.journal.last_hash.clone();
    let mut pipeline = fsm_execute::run::Pipeline;
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    let error = pipeline
        .claim_native(
            &mut store,
            &mut clock,
            fsm_store::store::ExecutionClaimRequest {
                instance_id: claim.effect().0,
                effect_id: claim.effect().1,
                handler_fingerprint: material
                    .get("handler_fingerprint")
                    .unwrap()
                    .as_str()
                    .unwrap(),
                retry: &retry,
                domain: &domain,
                request_id: "native-independent-competing-claim",
                expected_seq: None,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "exec/store");
    assert_eq!(
        error
            .details
            .as_ref()
            .unwrap()
            .get("code")
            .and_then(Value::as_str),
        Some("store/execution_owned")
    );
    assert_eq!(store.records.len(), records);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert_eq!(store.journal.last_hash, head);
    assert_eq!(
        store.current_execution_claim_hash(&claim).unwrap(),
        binding.get("journal_claim").unwrap().as_str().unwrap()
    );
    emit(format_args!("\nFSM_NATIVE_WRITER_READY"));
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
        Self::start_test(
            path,
            "authority::allocator::native_tests::supervisor_probe::writer_holder",
        )
    }

    fn lease_only(path: &str) -> Self {
        Self::start_test(
            path,
            "authority::allocator::native_tests::supervisor_probe::fresh_admission::writer_lease_only",
        )
    }

    fn start_test(path: &str, test: &str) -> Self {
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
                test,
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

#[test]
#[ignore = "invoked only as an unprivileged provisioned allocator subprocess"]
fn prepare_domain() {
    use fsm_execute::run::native_client::{NativePreparation, NativePreparationPhase};
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let namespace = std::env::var("FSM_NATIVE_TEST_NAMESPACE").unwrap();
    let mut cancelled = NativePreparation::start(&namespace, 1, Duration::from_secs(3)).unwrap();
    assert_eq!(
        cancelled.progress().phase,
        NativePreparationPhase::Preparing
    );
    cancelled.cancel().unwrap();
    assert!(cancelled.poll().is_err());
    let deadline = Instant::now() + Duration::from_secs(2);
    while !cancelled.reap().unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let progress = cancelled.progress();
    assert_eq!(progress.phase, NativePreparationPhase::Uncertain);
    assert!(progress.helper.reaped && progress.helper.stdout_eof && progress.helper.stderr_eof);
    let store =
        std::env::var("FSM_NATIVE_TEST_STORE").expect("discovery fixture requires physical store");
    let mut prepared =
        NativePreparation::for_store(std::path::Path::new(&store), Duration::from_secs(3)).unwrap();
    let domain = loop {
        if let Some(domain) = prepared.poll().unwrap() {
            break domain;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let progress = prepared.progress();
    assert_eq!(progress.phase, NativePreparationPhase::Prepared);
    assert!(progress.helper.reaped && progress.helper.stdout_eof && progress.helper.stderr_eof);
    assert!(prepared.poll().is_err());
    assert_eq!(prepared.progress(), progress);
    emit(format_args!(
        "\nFSM_NATIVE_TEST_DOMAIN={}",
        std::str::from_utf8(&fsm_core::canon::canon_bytes(&domain.to_value())).unwrap()
    ));
}

#[test]
#[ignore = "invoked only as an unprivileged provisioned cleanup subprocess"]
fn discard_prepared_domain() {
    use fsm_core::record::execution::NativeDomain;
    use fsm_execute::run::native_client::NativePreparedCleanup;
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let encoded = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    let domain =
        NativeDomain::from_value(&parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap())
            .unwrap();
    // A second fresh helper must replay the original domain retirement.
    for _ in 0..2 {
        let mut cleanup = NativePreparedCleanup::start(&domain, Duration::from_secs(3)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        while !cleanup.poll().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let progress = cleanup.progress();
        assert!(progress.reaped && progress.stdout_eof && progress.stderr_eof);
        cleanup.cancel().unwrap();
        assert!(cleanup.poll().unwrap());
        assert!(cleanup.reap().unwrap());
        assert_eq!(cleanup.progress(), progress);
    }
}

fn reject_run(claim: &Claim, hash: &str) {
    let mut owned = NativeRun::start(claim, hash, Duration::from_secs(30)).unwrap();
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
        emit(format_args!("\nFSM_NATIVE_TEST_CANCELLED"));
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
        emit(format_args!("\nFSM_NATIVE_TEST_REFUSED"));
        return;
    }
    panic!("negative native fixture requires cancellation or binding refusal");
}

// This test-only subprocess publishes bounded fixture barriers to its parent;
// production libraries never print, and stdout failures remain test failures.
fn emit(message: std::fmt::Arguments<'_>) {
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{message}").unwrap();
}

#[test]
#[ignore = "invoked only as an unprivileged provisioned discovery subprocess"]
fn refuse_discovery() {
    use fsm_execute::run::native_client::NativePreparation;
    let store = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let expected = std::env::var("FSM_NATIVE_TEST_DISCOVERY_ERROR").unwrap();
    let result = NativePreparation::for_store(std::path::Path::new(&store), Duration::from_secs(3));
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("faulted discovery started preparation"),
    };
    assert_eq!(error, expected);
}

#[test]
#[ignore = "invoked only as the unprivileged shared tick native control"]
fn shared_tick_recovery() {
    use fsm_execute::{
        config::{HandlerKind, HandlerSpec, HandlerTable, Retry},
        run::{Pipeline, Runner},
        sched::{Directive, Scheduler},
        service::{tick_reporting, tick_with},
        watch::{Observation, Watcher},
    };
    use fsm_store::{clock::FixedClock, store::Store};
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let path = std::path::Path::new(&path);
    let binding = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    let binding = parse(binding.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let (instance, effect) = claim.effect();
    let original = Store::open_read_only(path).unwrap();
    let records = original.records.clone();
    let state = original.state.clone();
    let pending = fsm_execute::effect::resolve(&original, effect).unwrap();
    drop(original);
    // Create a local reservation without running its deliberately different
    // current handler; only the immutable original claim can settle this run.
    let mut local_table = HandlerTable {
        max_inflight: 1,
        ..HandlerTable::default()
    };
    local_table.handlers.insert(
        pending.effect_name.clone(),
        HandlerSpec {
            effect: pending.effect_name.clone(),
            kind: HandlerKind::Process,
            argv: vec!["/bin/false".into()],
            timeout_ms: 30000,
            on_ok: None,
            on_failed: None,
            retry: Retry::default(),
        },
    );
    let mut local_scheduler = Scheduler::new(local_table);
    assert!(matches!(
        local_scheduler
            .on_observation(
                &Observation {
                    pending: vec![pending.clone()],
                    ..Observation::default()
                },
                1000
            )
            .as_slice(),
        [Directive::Start { .. }]
    ));
    assert!(local_scheduler.retain_claim(&claim));
    let table = HandlerTable::default();
    let mut watcher = Watcher::with_handlers(path.into(), &table);
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(1000, 1);
    let mut holder = WriterHolder::start(path.to_str().unwrap());
    holder.acquire();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            Instant::now() < deadline,
            "shared completion readiness deadline"
        );
        assert!(holder.child.try_wait().unwrap().is_none());
        let outcome = tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            path,
            &mut clock,
            1000,
        );
        let snapshot = Store::open_read_only(path).unwrap();
        assert_eq!(snapshot.records, records);
        assert!(fsm_store::snapshot::store_states_eq(
            &snapshot.state,
            &state
        ));
        if outcome.writer_unavailable {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut readonly = Store::open_read_only(path).unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut readonly,
        &mut clock,
        1000,
    );
    assert!(
        lines.iter().any(|line| line.contains("exec/mode")),
        "{lines:?}"
    );
    assert_eq!(readonly.records, records);
    assert!(fsm_store::snapshot::store_states_eq(
        &readonly.state,
        &state
    ));
    let local_lines = tick_with(
        &mut watcher,
        &mut local_scheduler,
        &mut runner,
        &mut pipeline,
        &mut readonly,
        &mut clock,
        1000,
    );
    assert!(
        local_lines.iter().any(|line| line.contains("exec/mode")),
        "{local_lines:?}"
    );
    assert_eq!(local_scheduler.inflight_effect(effect), Some(&pending));
    assert_eq!(readonly.records, records);
    drop(readonly);
    holder.release();
    let mut writer = Store::open(path).unwrap();
    writer
        .send_event(
            instance,
            "suspend",
            Value::Obj(BTreeMap::new()),
            "shared-suspend",
            None,
        )
        .unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut local_scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("disposition=acked advance=deferred")),
        "{lines:?}"
    );
    assert!(writer.state.execution.claim_for(instance, effect).is_none());
    assert!(local_scheduler.inflight_effect(effect).is_none());
    assert!(
        !writer.state.instances[instance]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
    let prefix = (writer.journal.last_seq, writer.journal.last_hash.clone());
    let settled = writer.records.clone();
    for _ in 0..3 {
        tick_with(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            &mut writer,
            &mut clock,
            1000,
        );
    }
    assert_eq!(writer.records, settled);
    assert_eq!(
        (writer.journal.last_seq, writer.journal.last_hash.clone()),
        prefix
    );
    writer
        .send_event(
            instance,
            "resume",
            Value::Obj(BTreeMap::new()),
            "shared-resume",
            None,
        )
        .unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("disposition=acked advance=advanced")),
        "{lines:?}"
    );
    let ack = fsm_execute::rid::ack_rid(effect);
    let event = fsm_execute::rid::event_rid(effect, "docs_ok");
    let ack_record = writer
        .records
        .iter()
        .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(&ack))
        .unwrap();
    let event_record = writer
        .records
        .iter()
        .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(&event))
        .unwrap();
    assert!(ack_record.seq < event_record.seq);
    let count = writer.records.len();
    tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert_eq!(writer.records.len(), count);
    drop(writer);
    let reopened = Store::open_read_only(path).unwrap();
    assert!(
        reopened
            .state
            .execution
            .claim_for(instance, effect)
            .is_none()
    );
    assert!(reopened.state.dedup.contains_key(&ack) && reopened.state.dedup.contains_key(&event));
    emit(format_args!("\nFSM_NATIVE_SHARED_RECOVERY"));
}

#[path = "supervisor_fresh_native_probe.rs"]
mod fresh_handoff;

#[path = "supervisor_admission_native_probe.rs"]
mod fresh_admission;
