//! Actual process death inside append; result evidence is preauthenticated fixture data.

use super::*;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn execution_append_child() {
    let Some(directory) = std::env::var_os("FSM_PRIVATE_APPEND_BARRIER_DIRECTORY") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let kind = std::env::var("FSM_PRIVATE_APPEND_BARRIER_KIND").unwrap();
    let (mut store, effect) = pending_store(Store::open(&directory).unwrap());
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    if kind == "execution_claimed" {
        panic!("claim returned beyond the selected append boundary");
    }
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    stop(&mut store, &claim, "timeout", "stop");
    if kind == "execution_stopped" {
        panic!("stop returned beyond the selected append boundary");
    }
    store
        .settle_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Attempted,
                request_id: "settle",
                expected_seq: None,
            },
        )
        .unwrap();
    panic!("settlement returned beyond the selected append boundary");
}

#[test]
fn death_inside_execution_append_recovers_the_complete_observable_prefix() {
    for kind in [
        "execution_claimed",
        "execution_stopped",
        "execution_settled",
    ] {
        for phase in ["before-write", "after-write", "after-sync"] {
            let directory = Directory(std::env::temp_dir().join(format!(
                "fsm-append-death-{}-{}-{kind}-{phase}", std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())));
            std::fs::create_dir_all(&directory.0).unwrap();
            let mut child = Process(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "store::execution::tests::append_durability::execution_append_child",
                        "--nocapture",
                    ])
                    .env("FSM_PRIVATE_APPEND_BARRIER_DIRECTORY", &directory.0)
                    .env("FSM_PRIVATE_APPEND_BARRIER_KIND", kind)
                    .env("FSM_PRIVATE_APPEND_BARRIER_PHASE", phase)
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let marker = directory.0.join("append-barrier.json");
            let deadline = Instant::now();
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "writer exited before {kind}/{phase}"
                );
                assert!(
                    deadline.elapsed() < Duration::from_secs(30),
                    "missing {kind}/{phase} barrier"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            let record = parse(&std::fs::read(&marker).unwrap(), &JsonLimits::DEFAULT).unwrap();
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            assert!(
                std::fs::read_dir(directory.0.join("snapshots"))
                    .unwrap()
                    .next()
                    .is_none()
            );
            let complete = phase != "before-write";
            let sequence: u64 = record
                .get("seq")
                .unwrap()
                .as_num()
                .unwrap()
                .parse()
                .unwrap();
            let head = sequence - u64::from(!complete);
            let hash = record
                .get(if complete { "hash" } else { "prev" })
                .unwrap()
                .as_str()
                .unwrap();
            let read_only = Store::open_read_only(&directory.0).unwrap();
            assert_eq!(read_only.journal.last_seq, head);
            assert_eq!(read_only.journal.last_hash, hash);
            let before = read_only.state.clone();
            drop(read_only);
            let mut store = Store::open(&directory.0).unwrap();
            assert!(crate::snapshot::store_states_eq(&store.state, &before));
            let effect = store.state.instances["instance"].pending[0].clone();
            let high_water = u64::from(kind != "execution_claimed" || complete);
            assert_eq!(store.state.execution.run_high_water(), high_water);
            let request = match kind {
                "execution_claimed" => "claim",
                "execution_stopped" => "stop",
                _ => "settle",
            };
            assert_eq!(store.state.dedup.contains_key(request), complete);
            let settled = kind == "execution_settled" && complete;
            assert_eq!(store.attempts_for("instance", &effect), u64::from(settled));
            assert_eq!(
                store.state.execution.unresolved().count(),
                usize::from(high_water == 1 && !settled)
            );
            assert_eq!(
                store
                    .state
                    .execution
                    .stopped_for("instance", &effect)
                    .is_some(),
                (kind == "execution_stopped" && complete)
                    || (kind == "execution_settled" && !complete)
            );
            if settled {
                assert_eq!(
                    allocate(&mut store, &effect, "retry", &mut FixedClock::new(109, 1))
                        .unwrap_err()
                        .code,
                    "store/execution_retry"
                );
                allocate(&mut store, &effect, "retry", &mut FixedClock::new(110, 1)).unwrap();
                assert_eq!(store.state.execution.run_high_water(), 2);
            } else if high_water == 1 {
                assert_eq!(
                    allocate(&mut store, &effect, "overlap", &mut FixedClock::new(200, 1))
                        .unwrap_err()
                        .code,
                    "store/execution_owned"
                );
                assert_eq!(store.journal.last_seq, head);
                assert_eq!(store.journal.last_hash, hash);
            } else {
                // No claim API returned and no native launch was authorized;
                // an absent record permits the first allocation after reopen.
                allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
                assert_eq!(store.state.execution.run_high_water(), 1);
            }
            assert_fold(&store);
            assert!(matches!(
                crate::journal_io::verify(&directory.0).health,
                crate::journal_io::JournalHealth::Ok
            ));
        }
    }
}
