//! Native I/O cancellation is separate from process-tree termination evidence.
use std::fs;
use std::net::Shutdown;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{Probe, await_file, fixture};

#[test]
fn socket_read_cancellation_joins_with_surviving_descendant() {
    let directory = std::env::temp_dir().join(format!(
        "fsm-lifecycle-socket-cancel-{}",
        std::process::id()
    ));
    fs::create_dir(&directory).expect("unique pipe cancellation directory");
    let (mut reader, writer) = UnixStream::pair().expect("native stdout socket pair");
    let control = reader
        .try_clone()
        .expect("independent read cancellation handle");
    // This extra peer remains outside the child tree. Even killing the entire
    // tree would not produce EOF, so EOF cannot accidentally earn this proof.
    let retained_peer = writer.try_clone().expect("outside retained peer");
    let root = fixture(&directory, "kill")
        .env("FSM_LIFECYCLE_SOCKET_CANCEL_FIXTURE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::null())
        .spawn()
        .expect("real handler root");
    let mut probe = Probe { directory, root };
    let descriptor = reader.as_raw_fd();
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut worker = Some(std::thread::spawn(move || {
        // TID is diagnostic synchronization for an actual blocked syscall,
        // never process-tree ownership or permission to clear a claim.
        let status = fs::read_to_string("/proc/thread-self/status").expect("thread diagnostic");
        let tid = status
            .lines()
            .find_map(|line| line.strip_prefix("Pid:"))
            .expect("native thread ID")
            .trim()
            .to_owned();
        ready_sender.send(tid).expect("reader diagnostic");
        let result = std::io::copy(&mut reader, &mut std::io::sink());
        let _ = sender.send(result);
    }));
    await_file(&probe.directory.join("root-ready"));
    probe.root.kill().expect("direct root kill");
    probe.root.wait().expect("direct root reap");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let tid = ready_receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("reader started");
    let blocked = Instant::now();
    loop {
        let syscall = fs::read_to_string(format!("/proc/self/task/{tid}/syscall"))
            .expect("own blocked-reader diagnostic");
        if syscall.split_whitespace().nth(1) == Some(format!("0x{descriptor:x}").as_str()) {
            break;
        }
        assert!(
            blocked.elapsed() < Duration::from_secs(3),
            "reader did not block on its socket"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    // This is the load-bearing native cancellation operation, not fixture
    // cleanup: the descendant and outside peer are both still alive.
    control
        .shutdown(Shutdown::Read)
        .expect("native read cancellation");
    let cancelled = receiver.recv_timeout(Duration::from_millis(250));
    if cancelled.is_ok() {
        worker
            .take()
            .expect("reader handle")
            .join()
            .expect("cancelled reader joined");
    }
    let cancellation_elapsed = started.elapsed();
    fs::write(
        probe.directory.join("challenge"),
        b"fresh after cancellation",
    )
    .expect("independent liveness challenge");
    await_file(&probe.directory.join("response"));
    // Cooperative cleanup is deliberately after observing the candidate
    // cancellation result, and cannot turn a timeout into a passing proof.
    fs::write(probe.directory.join("stop"), b"fixture cleanup").expect("cleanup marker");
    await_file(&probe.directory.join("descendant-stopped"));
    drop(retained_peer);
    if cancelled.is_err() {
        control
            .shutdown(Shutdown::Read)
            .expect("failure cleanup cancellation");
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded failure cleanup")
            .expect("cleanup reader result");
    }
    if let Some(worker) = worker {
        worker.join().expect("failure cleanup reader joined");
    }
    fs::remove_dir_all(&probe.directory).expect("fixture directory cleanup");
    cancelled
        .expect("read cancellation exceeded bound")
        .expect("cancelled reader result");
    assert!(cancellation_elapsed < Duration::from_millis(250));
}
