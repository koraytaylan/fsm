//! Real child processes exercise final delivery without synthetic drainage.
use super::*;
use std::{
    io::{BufRead, Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Child, Command, Stdio},
};

const CHILD: &str = "native_error::tests::renderer_child";

fn failure(large: bool) -> NativeSessionFailure {
    NativeSessionFailure {
        error: ExecError::new(
            "exec/inflight_deferred",
            if large {
                "x".repeat(2 * 1024 * 1024)
            } else {
                "original failure".into()
            },
        )
        .hint("original hint")
        .details(fsm_core::json::Value::Bool(false)),
        deadline: Instant::now() + Duration::from_millis(500),
    }
}

#[test]
fn renderer_child() {
    let Ok(mode) = std::env::var("FSM_FINAL_ERROR_TEST") else {
        return;
    };
    let ctx = Ctx::new("unused".into(), mode.ends_with("json"), false);
    let failure = failure(mode.starts_with("large"));
    writeln!(io::stdout(), "RENDER_READY").unwrap();
    io::stdout().flush().unwrap();
    std::process::exit(i32::from(report_until(&ctx, &failure)));
}

fn command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD, "--nocapture"])
        .env("FSM_FINAL_ERROR_TEST", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    command
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn blocked_exit(mut command: Command, socket: bool) {
    let mut child = Process(command.spawn().unwrap());
    let output = child.0.stdout.take().unwrap();
    let mut output = io::BufReader::new(output);
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(output.read_line(&mut line).unwrap(), 0);
        if line.trim_end().ends_with("RENDER_READY") {
            break;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut blocked = false;
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        if let Ok(entries) = std::fs::read_dir(format!("/proc/{}/task", child.0.id())) {
            blocked |= entries.filter_map(Result::ok).any(|entry| {
                std::fs::read_to_string(entry.path().join("comm"))
                    .is_ok_and(|name| name.starts_with("fsm-protocol-o"))
                    && std::fs::read_to_string(entry.path().join("wchan")).is_ok_and(|wait| {
                        if socket {
                            wait.contains("sock") || wait.contains("unix")
                        } else {
                            wait.contains("pipe")
                        }
                    })
            });
        }
        assert!(
            Instant::now() < deadline,
            "final fallback blocked process exit"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(blocked, "actual final diagnostic worker was never blocked");
    assert_eq!(status.code(), Some(1));
}

#[test]
fn large_final_pipe_frame_exits_with_reader_open_and_unread() {
    let mut command = command("large-json");
    command.stderr(Stdio::piped());
    blocked_exit(command, false);
}

#[test]
fn final_socket_fallback_exits_with_reader_open_and_unread() {
    let (reader, writer) = UnixStream::pair().unwrap();
    let descriptor: OwnedFd = writer.into();
    let mut command = command("large-json");
    command.stderr(Stdio::from(descriptor));
    blocked_exit(command, true);
    drop(reader);
}

#[test]
fn healthy_final_json_and_human_frames_preserve_exact_error_bytes() {
    for mode in ["small-json", "small-human", "large-json", "large-human"] {
        let output = command(mode).stderr(Stdio::piped()).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let failure = failure(mode.starts_with("large"));
        let error = ErrorObj::new(failure.error.code, failure.error.message)
            .hint("original hint")
            .details(fsm_core::json::Value::Bool(false));
        let mut expected = Vec::new();
        crate::render::write_error(mode.ends_with("json"), false, &error, &mut expected);
        assert_eq!(output.stderr, expected);
    }
    // A healthy nonpipe descriptor exercises the same fallback without loss.
    let (mut reader, writer) = UnixStream::pair().unwrap();
    let descriptor: OwnedFd = writer.into();
    let read = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let output = command("small-human")
        .stderr(Stdio::from(descriptor))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error = ErrorObj::new("exec/inflight_deferred", "original failure")
        .hint("original hint")
        .details(fsm_core::json::Value::Bool(false));
    let mut expected = Vec::new();
    crate::render::write_error(false, false, &error, &mut expected);
    assert_eq!(read.join().unwrap(), expected);
}
