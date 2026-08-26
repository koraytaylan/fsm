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
    std::env::var_os("CARGO_BIN_EXE_fsm")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            // The fallback has to spell the executable the way the platform
            // does; on Windows the binary is `fsm.exe`, so a bare `fsm` never
            // exists and the test reports the binary as missing.
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug")
                .join(format!("fsm{}", std::env::consts::EXE_SUFFIX))
        })
}
