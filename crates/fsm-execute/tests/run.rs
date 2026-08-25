//! The runner is the only component that spawns processes, so every row here
//! is about a real child: what it wrote, how it ended, and what is left behind
//! afterwards.
//!
//! The stub handler is **this test binary, re-executed** with a marker
//! argument — the precedent `crash_harness.rs` sets. CI runs the whole suite
//! on Windows as a full test leg, so a `.sh` fixture would not be a fixture, it
//! would be a red job; re-execution needs no shell, no exec bit, and no extra
//! artifact.
//!
//! One consequence shapes the assertions: the test harness writes its own
//! `running 1 test` banner to the child's **stdout** before the stub runs, so
//! stdout rows assert on the tail of the capture, while stderr — which the
//! harness leaves alone — carries the byte-exact, cap, and digest rows.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use fsm_core::json::Value;
use fsm_core::sha256::{sha256, to_hex};
use fsm_execute::run::{ACK_OUTPUT_CAP, BoundedBytes, KillReason, RunOutcome, Runner};

const STDOUT_MARKER: &str = "handler-stdout-line\n";
const STDERR_MARKER: &str = "handler-stderr-line\n";
const BIG_STREAM_BYTES: usize = 256 * 1024;

/// The stub handler.
///
/// In an ordinary `cargo test` run this returns immediately. When the parent
/// re-executes this binary it passes `stub:<mode>` as an extra filter
/// argument, which the harness ignores (no test is named that) and this
/// function reads.
#[test]
fn stub_handler() {
    let Some(mode) = std::env::args().find_map(|argument| {
        argument
            .strip_prefix("stub:")
            .map(std::string::ToString::to_string)
    }) else {
        return;
    };
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    match mode.as_str() {
        "ok" | "exit3" => {
            let _ = stdout.write_all(STDOUT_MARKER.as_bytes());
            let _ = stdout.flush();
            let _ = stderr.write_all(STDERR_MARKER.as_bytes());
            let _ = stderr.flush();
            std::process::exit(if mode == "ok" { 0 } else { 3 });
        }
        "sleep" => {
            std::thread::sleep(std::time::Duration::from_secs(120));
            std::process::exit(0);
        }
        "big" => {
            let _ = stderr.write_all(&big_stream());
            let _ = stderr.flush();
            std::process::exit(0);
        }
        "binary" => {
            let _ = stderr.write_all(&binary_stream());
            let _ = stderr.flush();
            std::process::exit(0);
        }
        _ => std::process::exit(97),
    }
}

/// Well past any OS pipe buffer, so a piped implementation would deadlock here.
fn big_stream() -> Vec<u8> {
    (0..BIG_STREAM_BYTES)
        .map(|index| b'a' + (index % 26) as u8)
        .collect()
}

/// A byte no UTF-8 decoder accepts, then a two-byte character deliberately
/// straddling the capture cap.
fn binary_stream() -> Vec<u8> {
    let mut bytes = vec![0x80];
    bytes.extend(std::iter::repeat_n(b'a', ACK_OUTPUT_CAP - 2));
    bytes.extend_from_slice("é".as_bytes());
    bytes.extend(std::iter::repeat_n(b'z', 64));
    bytes
}

fn stub_argv(mode: &str) -> Vec<String> {
    vec![
        std::env::current_exe()
            .expect("the test binary knows its own path")
            .to_string_lossy()
            .into_owned(),
        "stub_handler".into(),
        "--exact".into(),
        "--nocapture".into(),
        format!("stub:{mode}"),
    ]
}

/// Reap a child that is expected to finish on its own.
fn wait_for_outcome(runner: &mut Runner, effect_id: &str) -> RunOutcome {
    for _ in 0..2_000 {
        if let Some(outcome) = runner.poll(effect_id) {
            return outcome;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the stub never exited");
}

fn capture_files(runner: &Runner) -> Vec<PathBuf> {
    fs::read_dir(runner.scratch_dir())
        .expect("the scratch directory exists")
        .map(|entry| entry.expect("readable entry").path())
        .collect()
}

#[test]
fn a_clean_exit_reports_status_zero_with_both_streams_captured() {
    let mut runner = Runner::new().unwrap();
    runner.spawn("case-1/3/0".into(), &stub_argv("ok")).unwrap();
    match wait_for_outcome(&mut runner, "case-1/3/0") {
        RunOutcome::Completed {
            status,
            stdout,
            stderr,
        } => {
            assert_eq!(status, 0);
            assert!(
                stdout.to_json_string().ends_with(STDOUT_MARKER),
                "captured stdout was {:?}",
                stdout.to_json_string()
            );
            assert_eq!(stderr.to_json_string(), STDERR_MARKER);
            assert!(!stdout.truncated);
            assert!(!stderr.truncated);
            assert_eq!(stderr.sha256, None);
        }
        other => panic!("expected a completion, got {other:?}"),
    }
}

#[test]
fn a_non_zero_exit_is_reported_verbatim() {
    let mut runner = Runner::new().unwrap();
    runner
        .spawn("case-1/3/0".into(), &stub_argv("exit3"))
        .unwrap();
    match wait_for_outcome(&mut runner, "case-1/3/0") {
        RunOutcome::Completed { status, .. } => assert_eq!(status, 3),
        other => panic!("expected a completion, got {other:?}"),
    }
}

#[test]
fn a_command_that_does_not_exist_fails_to_spawn_and_records_no_child() {
    let mut runner = Runner::new().unwrap();
    let missing = if cfg!(windows) {
        r"C:\fsm\no\such\handler.exe"
    } else {
        "/nonexistent/fsm-handler"
    };
    let error = runner
        .spawn("case-1/3/0".into(), &[missing.to_string()])
        .unwrap_err();
    assert_eq!(error.code, "exec/spawn");
    assert!(error.message.contains(missing), "{error:?}");
    assert!(runner.running_effects().is_empty());
    assert!(runner.poll("case-1/3/0").is_none());
}

#[test]
fn an_empty_argv_is_refused_rather_than_indexed_into() {
    let mut runner = Runner::new().unwrap();
    let error = runner.spawn("case-1/3/0".into(), &[]).unwrap_err();
    assert_eq!(error.code, "exec/spawn");
}

#[test]
fn a_killed_run_reports_the_reason_it_was_given_and_leaves_no_child() {
    for reason in [KillReason::Timeout, KillReason::Cancelled] {
        let mut runner = Runner::new().unwrap();
        runner
            .spawn("case-1/3/0".into(), &stub_argv("sleep"))
            .unwrap();
        assert_eq!(runner.running_effects(), ["case-1/3/0"]);
        let outcome = runner.kill("case-1/3/0", reason);
        assert_eq!(outcome, RunOutcome::Killed { reason });
        assert!(
            runner.running_effects().is_empty(),
            "the child is reaped, not left a zombie"
        );
        assert!(capture_files(&runner).is_empty(), "capture files are gone");
    }
}

#[test]
fn an_ack_result_is_deterministic_and_carries_no_varying_field() {
    let mut runner = Runner::new().unwrap();
    runner.spawn("case-1/3/0".into(), &stub_argv("ok")).unwrap();
    let completed = wait_for_outcome(&mut runner, "case-1/3/0");
    assert_eq!(completed.ack_result(), completed.ack_result());
    let result = completed.ack_result();
    assert_eq!(result.get("status"), Some(&Value::Num("0".into())));

    let timed_out = RunOutcome::Killed {
        reason: KillReason::Timeout,
    };
    assert_eq!(
        timed_out.ack_result().get("error"),
        Some(&Value::Str("exec/timeout".into()))
    );
    let cancelled = RunOutcome::Killed {
        reason: KillReason::Cancelled,
    };
    assert_eq!(
        cancelled.ack_result().get("error"),
        Some(&Value::Str("exec/cancelled".into()))
    );
    let failed = RunOutcome::SpawnFailed {
        argv0: "/nonexistent/fsm-handler".into(),
    };
    let failed_result = failed.ack_result();
    assert_eq!(
        failed_result.get("error"),
        Some(&Value::Str("exec/spawn".into()))
    );
    assert_eq!(
        failed_result.get("argv0"),
        Some(&Value::Str("/nonexistent/fsm-handler".into()))
    );
    assert_eq!(failed_result.get("status"), Some(&Value::Num("-1".into())));
    assert_eq!(failed.ack_result(), failed_result);
}

#[test]
fn output_past_the_cap_is_truncated_and_digested() {
    let mut runner = Runner::new().unwrap();
    runner
        .spawn("case-1/3/0".into(), &stub_argv("big"))
        .unwrap();
    match wait_for_outcome(&mut runner, "case-1/3/0") {
        RunOutcome::Completed { stderr, .. } => {
            assert!(stderr.truncated);
            assert!(stderr.bytes.len() <= ACK_OUTPUT_CAP);
            assert_eq!(stderr.bytes.len(), ACK_OUTPUT_CAP);
            assert_eq!(
                stderr.sha256.as_deref(),
                Some(to_hex(&sha256(&big_stream())).as_str()),
                "the digest covers the whole stream, not the capture"
            );
            assert_eq!(&stderr.bytes[..], &big_stream()[..ACK_OUTPUT_CAP]);
        }
        other => panic!("expected a completion, got {other:?}"),
    }
}

#[test]
fn a_handler_writing_past_the_pipe_buffer_still_completes() {
    // A piped implementation deadlocks here: the child blocks writing past the
    // OS pipe buffer (~64 KiB), and a runner that only reads after `try_wait`
    // never drains it. This row is the reason output goes to files.
    const _: () = assert!(BIG_STREAM_BYTES > 64 * 1024);
    let mut runner = Runner::new().unwrap();
    runner
        .spawn("case-1/3/0".into(), &stub_argv("big"))
        .unwrap();
    match wait_for_outcome(&mut runner, "case-1/3/0") {
        RunOutcome::Completed { status, stderr, .. } => {
            assert_eq!(status, 0);
            assert!(stderr.sha256.is_some());
        }
        other => panic!("expected a completion, got {other:?}"),
    }
}

#[test]
fn invalid_utf8_renders_lossily_and_truncates_on_a_character_boundary() {
    let mut runner = Runner::new().unwrap();
    runner
        .spawn("case-1/3/0".into(), &stub_argv("binary"))
        .unwrap();
    match wait_for_outcome(&mut runner, "case-1/3/0") {
        RunOutcome::Completed { stderr, .. } => {
            let rendered = stderr.to_json_string();
            assert!(
                rendered.starts_with('\u{FFFD}'),
                "a byte the handler really wrote survives as a replacement char"
            );
            assert!(
                rendered.ends_with('a'),
                "the half-written character at the cap is dropped, not rendered: {:?}",
                &rendered[rendered.len().saturating_sub(8)..]
            );
            assert_eq!(
                stderr.sha256.as_deref(),
                Some(to_hex(&sha256(&binary_stream())).as_str()),
                "the digest still covers the true bytes"
            );
        }
        other => panic!("expected a completion, got {other:?}"),
    }
}

#[test]
fn a_reaped_run_leaves_no_capture_files_behind() {
    let mut runner = Runner::new().unwrap();
    runner.spawn("case-1/3/0".into(), &stub_argv("ok")).unwrap();
    wait_for_outcome(&mut runner, "case-1/3/0");
    assert!(runner.running_effects().is_empty());
    assert!(capture_files(&runner).is_empty());
}

#[test]
fn dropping_the_runner_kills_a_live_child_and_removes_its_directory() {
    let scratch;
    {
        let mut runner = Runner::new().unwrap();
        scratch = runner.scratch_dir().to_path_buf();
        runner
            .spawn("case-1/3/0".into(), &stub_argv("sleep"))
            .unwrap();
        assert!(scratch.is_dir());
    }
    assert!(
        !scratch.exists(),
        "a clean shutdown leaves nothing behind; only a signalled one orphans"
    );
}

#[test]
fn two_runners_in_one_process_own_separate_directories() {
    // The chaos harness restarts an executor by building a fresh runner while
    // the old one is still alive; a shared scratch directory would be removed
    // out from under the new one.
    let first = Runner::new().unwrap();
    let second = Runner::new().unwrap();
    assert_ne!(first.scratch_dir(), second.scratch_dir());
    let kept = first.scratch_dir().to_path_buf();
    drop(second);
    assert!(kept.is_dir());
}

#[test]
fn a_bounded_capture_of_nothing_is_an_empty_string() {
    let empty = BoundedBytes::empty();
    assert_eq!(empty.to_json_string(), "");
    assert!(!empty.truncated);
    assert_eq!(empty.sha256, None);
}
