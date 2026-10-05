//! Production capture checks; these do not establish native domain closure.

#[test]
fn protected_entry_does_not_accept_a_caller_supplied_command() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fsm-containment-authority"))
        .args(["gate", &"a".repeat(32), "1", "1", "/bin/true"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    assert!(stderr.contains("gate requires namespace") || stderr.contains("isolated unprivileged"));
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    assert!(stderr.contains("native containment runtime unsupported"));
}

#[test]
fn authority_refuses_unprivileged_or_unsupported_before_request_io() {
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use std::os::unix::fs::MetadataExt;
        if std::fs::metadata("/proc/self").unwrap().uid() == 0 {
            return; // Privileged publication requires its separate native gate.
        }
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fsm-containment-authority"))
        .args([
            "bind",
            &"a".repeat(32),
            "1",
            "/nonexistent/fsm-authority-request",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    assert!(stderr.contains("requires separately provisioned root authority"));
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    assert!(stderr.contains("native containment runtime unsupported"));
}

#[cfg(target_os = "linux")]
mod linux {
    use fsm_core::sha256::{sha256, to_hex};
    use fsm_execute::run::{ACK_OUTPUT_CAP, MAX_CAPTURE_READ_BYTES, RunOutcome, Runner};
    use std::io::Write;
    use std::time::{Duration, Instant};

    #[test]
    fn capture_fixture() {
        let Some(size) = std::env::args().find_map(|arg| {
            arg.strip_prefix("capture-size:")
                .and_then(|size| size.parse::<usize>().ok())
        }) else {
            return;
        };
        let chunk = [b'x'; 8192];
        let mut remaining = size;
        while remaining > 0 {
            let count = remaining.min(chunk.len());
            std::io::stdout().write_all(&chunk[..count]).unwrap();
            std::io::stderr().write_all(&chunk[..count]).unwrap();
            remaining -= count;
        }
        std::process::exit(0);
    }

    #[test]
    fn real_noisy_roots_drain_beyond_hash_limit_without_spooling() {
        for size in [
            ACK_OUTPUT_CAP,
            ACK_OUTPUT_CAP + 1,
            MAX_CAPTURE_READ_BYTES,
            MAX_CAPTURE_READ_BYTES + 1,
            MAX_CAPTURE_READ_BYTES * 8,
        ] {
            let mut runner = Runner::new().unwrap();
            let argv = vec![
                std::env::current_exe().unwrap().display().to_string(),
                "--exact".into(),
                "linux::capture_fixture".into(),
                "--nocapture".into(),
                format!("capture-size:{size}"),
            ];
            runner.spawn("capture".into(), &argv, None).unwrap();
            let started = Instant::now();
            let outcome = loop {
                assert_eq!(std::fs::read_dir(runner.scratch_dir()).unwrap().count(), 0);
                // Exercise the scheduler's pre-writer observation path too.
                runner.finished_effects();
                if let Some(outcome) = runner.poll("capture") {
                    break outcome;
                }
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(1));
            };
            let RunOutcome::Completed {
                status,
                stdout,
                stderr,
            } = outcome
            else {
                panic!("unexpected capture outcome: {outcome:?}");
            };
            assert_eq!(status, 0);
            assert!(stdout.bytes.len() <= ACK_OUTPUT_CAP);
            assert_eq!(stderr.bytes, vec![b'x'; size.min(ACK_OUTPUT_CAP)]);
            assert_eq!(stderr.truncated, size > ACK_OUTPUT_CAP);
            assert_eq!(
                stderr.sha256,
                (size > ACK_OUTPUT_CAP && size <= MAX_CAPTURE_READ_BYTES)
                    .then(|| to_hex(&sha256(&vec![b'x'; size])))
            );
            assert!(runner.running_effects().is_empty());
        }
    }
}
