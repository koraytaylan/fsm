//! Owned protocol worker with independent Linux I/O cancellation controls.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;

use fsm_core::json::Value;

use crate::mcp_client::{McpOutcome, ProtocolFault, converse};

#[cfg(target_os = "linux")]
use std::{net::Shutdown, os::fd::OwnedFd, os::unix::net::UnixStream};

pub(super) struct McpWorker {
    answers: Receiver<McpOutcome>,
    answer: Option<McpOutcome>,
    thread: Option<JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    controls: Vec<UnixStream>,
}

impl McpWorker {
    pub(super) fn spawn(
        command: &str,
        arguments: &[String],
        stderr: Stdio,
        tool: String,
        call_arguments: Value,
    ) -> std::io::Result<(Child, Self)> {
        let mut builder = Command::new(command);
        builder.args(arguments).stderr(stderr);
        #[cfg(target_os = "linux")]
        let (stdin, stdout, controls) = {
            let (stdin, input_peer) = UnixStream::pair()?;
            let (stdout, output_peer) = UnixStream::pair()?;
            let controls = vec![stdin.try_clone()?, stdout.try_clone()?];
            builder
                .stdin(Stdio::from(OwnedFd::from(input_peer)))
                .stdout(Stdio::from(OwnedFd::from(output_peer)));
            (stdin, stdout, controls)
        };
        #[cfg(not(target_os = "linux"))]
        builder.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = builder.spawn()?;
        #[cfg(not(target_os = "linux"))]
        let (stdin, stdout) = match (child.stdin.take(), child.stdout.take()) {
            (Some(stdin), Some(stdout)) => (stdin, stdout),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::other("server pipes unavailable"));
            }
        };
        let (sender, answers) = channel();
        let thread = std::thread::Builder::new()
            .name("fsm-mcp".into())
            .spawn(move || {
                let outcome = exchange(stdin, stdout, &tool, &call_arguments);
                let _ = sender.send(outcome);
            });
        let thread = match thread {
            Ok(thread) => thread,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok((
            child,
            Self {
                answers,
                answer: None,
                thread: Some(thread),
                #[cfg(target_os = "linux")]
                controls,
            },
        ))
    }

    pub(super) fn cancel(&self) {
        #[cfg(target_os = "linux")]
        for control in &self.controls {
            // Cancellation failure is not process-domain closure evidence;
            // the unfinished worker remains retained by the runner.
            let _ = control.shutdown(Shutdown::Both);
        }
    }

    /// Only join a handle which has already finished: no tick waits on I/O.
    pub(super) fn reap(&mut self) -> bool {
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            return false;
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        true
    }

    pub(super) fn collect(&mut self) -> Option<McpOutcome> {
        if self.answer.is_none() {
            match self.answers.try_recv() {
                Ok(answer) => self.answer = Some(answer),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.answer = Some(McpOutcome::Protocol(ProtocolFault::Closed));
                }
            }
        }
        if !self.reap() {
            return None;
        }
        self.answer.take()
    }
}

impl Drop for McpWorker {
    fn drop(&mut self) {
        self.cancel();
        self.reap();
    }
}

fn exchange(stdin: impl Write, stdout: impl Read, tool: &str, arguments: &Value) -> McpOutcome {
    converse(stdin, stdout, tool, arguments)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    #[test]
    fn unjoined_worker_retains_launch_exclusion_until_observed_join() {
        let (mut reader, peer) = UnixStream::pair().unwrap();
        let (sender, answers) = channel();
        let thread = std::thread::spawn(move || {
            let _ = reader.read(&mut [0; 1]);
            let _ = sender.send(McpOutcome::Protocol(ProtocolFault::Closed));
        });
        // Inject unavailable cancellation controls, rather than claim a
        // simulated failure as native shutdown evidence.
        let worker = McpWorker {
            answers,
            answer: None,
            thread: Some(thread),
            controls: Vec::new(),
        };
        let mut runner = super::super::Runner::new().unwrap();
        runner.retiring.push(worker);
        let argv = vec!["/bin/true".into()];
        assert_eq!(
            runner.spawn("next".into(), &argv, None).unwrap_err().code,
            "exec/spawn"
        );
        assert!(runner.running_effects().is_empty());
        assert_eq!(runner.retiring.len(), 1);
        drop(peer);
        let started = Instant::now();
        while !runner.retiring.is_empty() {
            runner.finished_effects();
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(1));
        }
        runner.spawn("next".into(), &argv, None).unwrap();
        while runner.poll("next").is_none() {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn retained_peers_cannot_prevent_cancelled_reads_or_writes_from_joining() {
        for blocked_write in [false, true] {
            let (stdin, input_peer) = UnixStream::pair().unwrap();
            let (stdout, mut output_peer) = UnixStream::pair().unwrap();
            input_peer
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let controls = vec![stdin.try_clone().unwrap(), stdout.try_clone().unwrap()];
            let (sender, answers) = channel();
            let arguments = if blocked_write {
                Value::Str("x".repeat(2 * 1024 * 1024))
            } else {
                Value::Null
            };
            let thread = std::thread::spawn(move || {
                let outcome = exchange(stdin, stdout, "fixture", &arguments);
                let _ = sender.send(outcome);
            });
            let mut worker = McpWorker {
                answers,
                answer: None,
                thread: Some(thread),
                controls,
            };
            let mut input_peer = BufReader::new(input_peer);
            let mut line = String::new();
            input_peer.read_line(&mut line).unwrap();
            assert!(line.contains("initialize"));
            if blocked_write {
                output_peer
                    .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n")
                    .unwrap();
                line.clear();
                input_peer.read_line(&mut line).unwrap();
                assert!(line.contains("notifications/initialized"));
            }
            assert!(worker.collect().is_none());
            let started = Instant::now();
            worker.cancel();
            while !worker.reap() {
                assert!(started.elapsed() < Duration::from_secs(2));
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(worker.thread.is_none());
            assert!(matches!(worker.collect(), Some(McpOutcome::Protocol(_))));
            // Both peers survive the cancellation observation: EOF caused by
            // closing peers or killing their process cannot earn this proof.
            drop(input_peer);
            drop(output_peer);
        }
    }
}
