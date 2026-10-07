//! Kill an independent public-pipeline caller after its durable stopped append.

use super::*;
use std::io::{Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};

struct Host(Child);

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if !matches!(self.0.try_wait(), Ok(None)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

pub(super) fn kill_after_publication(fixture: &Fixture) {
    super::super::broker_cases::disconnect_cases::install_fixture_binary(
        &fixture.directory,
        "stopped-host-test",
    );
    let mut host = Host(
        Command::new(fixture.directory.join("stopped-host-test"))
            .args([
                "--exact",
                "authority::allocator::native_tests::runner_cases::stopped_host::persist_then_wait",
                "--ignored",
                "--nocapture",
            ])
            .env("FSM_STOPPED_HOST_STORE", &fixture.store)
            .env("FSM_STOPPED_HOST_AUTHORITY", &fixture.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.directory.join("stopped-host-ready").exists() {
        assert!(
            host.0.try_wait().unwrap().is_none(),
            "stopped host exited early"
        );
        assert!(
            Instant::now() < deadline,
            "stopped host publication deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(host.0.try_wait().unwrap().is_none());
    assert!(matches!(Store::open(&fixture.store), Err(error) if error.code == "store/lock"));
    host.0.kill().unwrap();
    loop {
        if let Some(status) = host.0.try_wait().unwrap() {
            assert_eq!(status.signal(), Some(9));
            break;
        }
        assert!(Instant::now() < deadline, "stopped host kill deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "started only as the independent stopped-record host by the native fixture"]
fn persist_then_wait() {
    use fsm_core::record::execution::Claim;
    use fsm_execute::run::{Pipeline, native_client::NativeCompletion};

    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let directory = PathBuf::from(std::env::var_os("FSM_STOPPED_HOST_AUTHORITY").unwrap());
    let path = PathBuf::from(std::env::var_os("FSM_STOPPED_HOST_STORE").unwrap());
    let binding = read_value(&directory.join("binding-1.json"), true).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let mut writer = Store::open(&path).unwrap();
    let hash = writer.current_execution_claim_hash(&claim).unwrap();
    let response = object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(true)),
        ("result", runner::recover(&directory, 1).unwrap()),
    ]);
    let completion = NativeCompletion::verify(&response, &claim, &hash).unwrap();
    Pipeline
        .stop_native(
            &mut writer,
            &mut fsm_store::clock::FixedClock::new(1000, 1),
            &claim,
            &completion,
            &format!("exec-stop-{}-{}", claim.effect().1, claim.run_id()),
        )
        .unwrap();
    let mut ready = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("stopped-host-ready"))
        .unwrap();
    ready.write_all(b"durable stopped append").unwrap();
    ready.sync_all().unwrap();
    let mut release = [0];
    std::io::stdin().read_exact(&mut release).unwrap();
    panic!("the parent must kill this host before settlement");
}
