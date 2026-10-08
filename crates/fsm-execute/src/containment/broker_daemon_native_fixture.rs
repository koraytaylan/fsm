//! Own and retire only the broker child started by a disposable native fixture.

use super::super::super::super::number;
use super::read_value;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub(in super::super) struct Daemon(pub(super) Child);

impl Daemon {
    pub(in super::super) fn kill_and_wait(&mut self) {
        use std::os::unix::process::ExitStatusExt;
        self.0.kill().unwrap();
        assert_eq!(self.0.wait().unwrap().signal(), Some(9));
    }

    fn start(directory: &Path) -> Self {
        if std::env::var_os("FSM_NATIVE_WORKFLOW_UPGRADE").is_some() {
            super::super::workflow_cases::stage_artifact(
                &directory.join("broker-test"),
                "FSM_NATIVE_WORKFLOW_ORIGINAL_BROKER_ARTIFACT",
                "FSM_NATIVE_WORKFLOW_ORIGINAL_BROKER_SHA256",
            );
        } else {
            super::disconnect_cases::install_fixture_binary(directory, "broker-test");
        }
        Self(
            Command::new("/usr/bin/python3")
                .args([
                    "-c",
                    "import os,sys;os.umask(0o077);os.execv(sys.argv[1],sys.argv[1:])",
                ])
                .arg(directory.join("broker-test"))
                .args([
                    "--exact",
                    "authority::allocator::native_tests::broker_cases::frozen_broker",
                    "--ignored",
                    "--nocapture",
                ])
                .env("FSM_NATIVE_BROKER_DIRECTORY", directory)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    pub(in super::super) fn ready(directory: &Path, epoch: u64) -> Self {
        let mut daemon = Self::start(directory);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(
                daemon.0.try_wait().unwrap().is_none(),
                "broker exited before publication"
            );
            if read_value(&directory.join("broker/route.json"), true)
                .is_ok_and(|route| number(&route, "epoch").unwrap() == epoch)
            {
                return daemon;
            }
            assert!(
                Instant::now() < deadline,
                "broker route publication timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub(super) fn refused(directory: &Path) {
        let mut daemon = Self::start(directory);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = daemon.0.try_wait().unwrap() {
                assert!(!status.success());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "uncertain broker startup was not refused"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}
