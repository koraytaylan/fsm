//! Shared setup helpers used by the regression tests in the sibling modules.

use std::sync::{Mutex, MutexGuard};

use fsm_core::json::{JsonLimits, Value, parse};

pub(crate) static GATE: Mutex<()> = Mutex::new(());

pub(crate) fn gate() -> MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-reg-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
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
