//! Real original-claim binding waits only before mutation, within one budget.

use super::Fixture;
use fsm_core::json::Value;
use fsm_store::store::Store;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

pub(super) fn bind(fixture: &Fixture, binding: &Value) {
    identity_after_contention(fixture, binding);
    let directory = &fixture.directory;
    let before = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    let lock = super::super::super::authority_lock(directory).unwrap();
    let started = Instant::now();
    let refused = super::super::super::bind(directory, binding);
    let elapsed = started.elapsed();
    drop(lock);
    assert_eq!(refused.unwrap_err(), "authority busy");
    assert!(elapsed >= Duration::from_secs(2));
    assert!(elapsed < Duration::from_secs(3));
    assert!(!directory.join("binding-1.json").exists());
    assert_no_launch(fixture);
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        before
    );

    let lock = super::super::super::authority_lock(directory).unwrap();
    let original_directory = directory.clone();
    let original_binding = binding.clone();
    let (entered, entering) = mpsc::channel();
    let (returned, returning) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        entered.send(()).unwrap();
        returned
            .send(super::super::super::bind(
                &original_directory,
                &original_binding,
            ))
            .unwrap();
    });
    entering.recv_timeout(Duration::from_secs(1)).unwrap();
    let during_contention = returning.recv_timeout(Duration::from_millis(100));
    let waited = matches!(&during_contention, Err(mpsc::RecvTimeoutError::Timeout));
    let unpublished = !directory.join("binding-1.json").exists();
    // Retire the original worker even when a neutralized retry guard fails.
    drop(lock);
    let result = match during_contention {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            returning.recv_timeout(Duration::from_secs(2)).unwrap()
        }
        Err(error) => panic!("original binding worker disconnected: {error}"),
    };
    worker.join().unwrap();
    assert!(waited, "binding must wait for its original authority lock");
    assert!(
        unpublished,
        "binding must not publish while another actor owns the lock"
    );
    result.unwrap();
    assert_eq!(
        super::super::super::read_value(&directory.join("binding-1.json"), true).unwrap(),
        *binding
    );
    assert_no_launch(fixture);
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        before
    );
}

fn identity_after_contention(fixture: &Fixture, binding: &Value) {
    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    let directory = fixture.directory.clone();
    let original = binding.clone();
    let (contended, contention) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut signal = Some(contended);
        super::super::super::bind_with_contention_probe(&directory, &original, || {
            if let Some(signal) = signal.take() {
                signal.send(()).unwrap();
                resumed.recv_timeout(Duration::from_secs(1)).unwrap();
            }
        })
    });
    contention.recv_timeout(Duration::from_secs(1)).unwrap();
    let mut swapped = NamespaceSwap::new(&fixture.directory);
    drop(lock);
    resume.send(()).unwrap();
    let result = worker.join().unwrap();
    let replacement_files = fs::read_dir(&fixture.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    swapped.restore().unwrap();
    assert_eq!(result.unwrap_err(), "authority identity differs");
    assert_eq!(replacement_files, vec![std::ffi::OsString::from("LOCK")]);
    assert!(!fixture.directory.join("binding-1.json").exists());
    assert_no_launch(fixture);
}

pub(super) struct NamespaceSwap {
    directory: PathBuf,
    saved: PathBuf,
    replacement: Option<(u64, u64)>,
    restored: bool,
}

impl NamespaceSwap {
    pub(super) fn new(directory: &Path) -> Self {
        let saved = directory.with_extension("binding-contention-original");
        assert!(!saved.exists());
        fs::rename(directory, &saved).unwrap();
        let mut swap = Self {
            directory: directory.to_path_buf(),
            saved,
            replacement: None,
            restored: false,
        };
        fs::DirBuilder::new().mode(0o700).create(directory).unwrap();
        let metadata = fs::symlink_metadata(directory).unwrap();
        swap.replacement = Some((metadata.dev(), metadata.ino()));
        swap
    }

    pub(super) fn restore(&mut self) -> std::io::Result<()> {
        if self.restored {
            return Ok(());
        }
        match fs::symlink_metadata(&self.directory) {
            Ok(metadata) => {
                if Some((metadata.dev(), metadata.ino())) != self.replacement {
                    return Err(std::io::Error::other(
                        "replacement namespace identity changed",
                    ));
                }
                let entries = fs::read_dir(&self.directory)?.collect::<Result<Vec<_>, _>>()?;
                if entries.iter().any(|entry| entry.file_name() != "LOCK") {
                    return Err(std::io::Error::other(
                        "replacement namespace contains unexpected state",
                    ));
                }
                for entry in entries {
                    fs::remove_file(entry.path())?;
                }
                fs::remove_dir(&self.directory)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&self.saved, &self.directory)?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for NamespaceSwap {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn assert_no_launch(fixture: &Fixture) {
    for prefix in ["launch", "handoff", "entry", "closing", "closed"] {
        assert!(fs::symlink_metadata(fixture.directory.join(format!("{prefix}-1.json"))).is_err());
    }
}
