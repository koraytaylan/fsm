//! Fatal-hook subprocesses exercise the same public filter used by installed stdio.

use crate::run::native_client::{
    NativeRequest, filter_native_worker_panics,
    proof_worker::ProofWorker,
    startup::FixtureFactory,
    test_support,
    worker::{Budget, Scope, reserve_current},
};
use std::{
    os::unix::process::ExitStatusExt,
    process::{Command, Output},
    sync::Arc,
    time::{Duration, Instant},
};

fn probe(mode: &str) -> Output {
    let fixture = if mode == "adoption" {
        "run::native_client::worker::tests::native_worker_panic_after_actual_retirement_discards_the_published_response"
    } else {
        "run::native_client::unwind::tests::native_unwind_process_probe"
    };
    // Disable kernel core dumps before re-exec; no artifact path is selected.
    Command::new("sh")
        .args(["-c", "ulimit -c 0\nexec \"$@\"", "sh"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", fixture, "--nocapture"])
        .env("FSM_NATIVE_UNWIND_TEST", mode)
        .output()
        .unwrap()
}

#[test]
fn native_unwind_fatal_hook_allows_original_transport_panic_without_payload_disclosure() {
    let output = probe("transport");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private fixture panic payload"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private fixture panic payload"));
}

#[test]
fn native_unwind_fatal_hook_allows_original_proof_panic_without_fabricating_result() {
    let output = probe("proof");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private fixture panic payload"));
}

#[test]
fn native_unwind_fatal_hook_allows_transferred_helper_panic_after_actual_retirement() {
    let output = probe("adoption");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture panic after actual"));
}

#[test]
fn native_unwind_fatal_hook_discards_a_published_proof_after_worker_panic() {
    let output = probe("proof_published");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private fixture panic payload"));
}

#[test]
fn native_unwind_forged_worker_name_still_reaches_the_original_fatal_hook() {
    let output = probe("foreign");
    assert_eq!(output.status.signal(), Some(6));
}

#[test]
fn native_unwind_process_probe() {
    let Ok(mode) = std::env::var("FSM_NATIVE_UNWIND_TEST") else {
        return;
    };
    std::panic::set_hook(Box::new(filter_native_worker_panics(|_| {
        std::process::abort()
    })));
    if mode == "foreign" {
        // A swallowed unmarked panic returns normally, making the parent fail.
        let worker = std::thread::Builder::new()
            .name("fsm-native-transport".into())
            .spawn(|| panic!("private fixture panic payload"))
            .unwrap();
        let _ = worker.join();
        return;
    }
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let until = Instant::now() + Duration::from_secs(5);
    match mode.as_str() {
        "transport" => {
            let _factory = FixtureFactory::install(|_| panic!("private fixture panic payload"));
            let message = fsm_core::json::parse(
                br#"{"format":"fsm.native-request/1","action":"prepare","payload":null}"#,
                &fsm_core::json::JsonLimits::DEFAULT,
            )
            .unwrap();
            let mut request = NativeRequest::start(
                "00000000000000000000000000000000",
                1,
                &message,
                Duration::from_secs(1),
            )
            .unwrap();
            assert_eq!(
                test_support::receive(&mut request).unwrap_err(),
                "native transport worker panicked; ownership remains uncertain"
            );
            assert!(!request.reap().unwrap());
            let progress = request.progress();
            assert!(
                !progress.not_started
                    && !progress.reaped
                    && !progress.stdout_eof
                    && !progress.stderr_eof
            );
            assert_eq!(budget.reserved(), 1);
            drop(request);
            assert_eq!(budget.reserved(), 0);
        }
        "proof" | "proof_published" => {
            let ticket = reserve_current().unwrap().unwrap();
            let _after_result = (mode == "proof_published").then(|| {
                crate::run::native_client::proof_worker::FixtureHook::after_result(|| {
                    panic!("private fixture panic payload");
                })
            });
            let publish = mode == "proof_published";
            let mut worker = ProofWorker::<()>::start(
                move || {
                    if publish {
                        Ok(())
                    } else {
                        panic!("private fixture panic payload");
                    }
                },
                Arc::clone(&ticket),
                until,
            )
            .unwrap();
            let error = loop {
                match worker.poll() {
                    Ok(None) => {
                        assert!(Instant::now() < until);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Ok(Some(())) => panic!("worker panic fabricated a proof result"),
                    Err(error) => break error,
                }
            };
            assert_eq!(
                error,
                "native proof worker panicked; original ownership remains uncertain"
            );
            assert!(worker.reap());
            assert_eq!(budget.reserved(), 1);
            drop(worker);
            drop(ticket);
            assert_eq!(budget.reserved(), 0);
        }
        _ => panic!("unknown private probe mode"),
    }
}
