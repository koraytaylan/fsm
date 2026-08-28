//! Shared setup helpers used by the regression tests in the sibling modules.

use std::sync::{Mutex, MutexGuard};

use fsm_core::json::{JsonLimits, Value, parse};

pub(crate) static GATE: Mutex<()> = Mutex::new(());

pub(crate) fn gate() -> MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A scratch directory that removes itself. A suite that leaks one per run
/// exhausts a long-lived machine's tmpfs inodes long before it exhausts its
/// bytes, and the failure looks like a broken toolchain rather than a leaky
/// test.
pub(crate) struct Scratch(pub(crate) std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn tmp(tag: &str) -> Scratch {
    let p = std::env::temp_dir().join(format!(
        "fsm-reg-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

pub(crate) fn case() -> Value {
    parse(
        include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

pub(crate) fn fsm_bin() -> std::path::PathBuf {
    // `CARGO_BIN_EXE_fsm` is set by cargo when this test is *compiled*, not
    // when it runs, so the runtime lookup this used to do never found it and
    // every call took the fallback below. That fallback assumes the default
    // target directory, so the suite passed only where one was in use and
    // reported the binary missing anywhere a `build.target-dir` is
    // configured — a shared cache, a CI layout, anything but the default.
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_fsm"))
}
