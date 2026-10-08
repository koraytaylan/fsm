//! Public shutdown receipt verification after genuine transport retirement.

use super::*;
use fsm_execute::run::native_client::NativeShutdown;
use fsm_store::store::Store;
use std::{fs, path::Path};

#[test]
#[ignore = "invoked only as an unprivileged subprocess of native broker tests"]
fn refuse_another_journal_claim() {
    use std::os::unix::fs::MetadataExt;
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 65534);
    let binding = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    let binding = parse(binding.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let hash = binding.get("journal_claim").unwrap().as_str().unwrap();
    let store_path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let handshake = std::env::var("FSM_SHUTDOWN_PROOF_HANDSHAKE").unwrap();
    let handshake = Path::new(&handshake);
    let store = Store::open_read_only(Path::new(&store_path)).unwrap();
    let records = store.records.clone();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut shutdown = NativeShutdown::start(&store, &claim, Duration::from_secs(20)).unwrap();
    // Root paused the actual owned broker, so this sends the request without
    // racing into receipt verification before the corruption handshake.
    assert!(shutdown.poll().unwrap().is_none());
    fs::write(handshake.join("request-sent"), b"sent").unwrap();
    while !shutdown.reap().unwrap() {
        assert!(
            Instant::now() < deadline,
            "shutdown transport retirement deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(shutdown.progress().is_retired());
    fs::write(handshake.join("transport-retired"), b"retired").unwrap();
    wait(handshake, "corrupted", deadline);
    loop {
        match shutdown.poll() {
            Err(error) => {
                assert_eq!(error, "native shutdown receipt differs from original claim");
                break;
            }
            Ok(Some(_)) => panic!("native shutdown accepted closure for another journal claim"),
            Ok(None) => {
                assert!(Instant::now() < deadline, "shutdown refusal deadline");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    assert_eq!(
        Store::open_read_only(Path::new(&store_path))
            .unwrap()
            .records,
        records
    );
    fs::write(handshake.join("refused"), b"refused").unwrap();
    wait(handshake, "restored", deadline);
    let mut restored = NativeShutdown::start(&store, &claim, Duration::from_secs(10)).unwrap();
    loop {
        if let Some(proof) = restored.poll().unwrap() {
            assert!(proof.matches_claim(&claim, hash));
            break;
        }
        assert!(Instant::now() < deadline, "restored shutdown deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(restored.reap().unwrap());
    assert_eq!(
        Store::open_read_only(Path::new(&store_path))
            .unwrap()
            .records,
        records
    );
}

fn wait(directory: &Path, name: &str, deadline: Instant) {
    while !directory.join(name).exists() {
        assert!(
            Instant::now() < deadline,
            "shutdown proof handshake deadline: {name}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
